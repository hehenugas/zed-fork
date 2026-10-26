use crate::{
    commit_view::CommitView, create_tag_at_commit, git_graph::GitGraph, project_diff::ProjectDiff,
};
use editor::Editor;
use fs::Fs;
use git::{
    BuildCommitPermalinkParams, GitHostingProviderRegistry, Oid, parse_git_remote_url,
    repository::ResetMode,
};
use gpui::{
    Action, ClipboardItem, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    PromptLevel, SharedString, WeakEntity, Window, actions,
};
use menu::{Cancel, Confirm};
use project::{GIT_COMMAND_TASK_TAG, git_store::Repository};

use task::{TaskContext, TaskVariables, VariableName};
use ui::{Color, ContextMenu, ContextMenuEntry, IconName, IconPosition, prelude::*};
use util::ResultExt as _;
use workspace::{ModalView, Workspace, notifications::DetachAndPromptErr};

actions!(
    git_graph,
    [
        /// Copies the SHA of the selected commit to the clipboard.
        CopyCommitSha,
        /// Copies a tag from the selected commit to the clipboard.
        CopyCommitTag,
        /// Opens the commit view for the selected commit.
        OpenCommitView,
    ]
);

const COMMIT_TAG_LIST_WIDTH_IN_REMS: Rems = rems(10.);
pub(crate) const CUSTOM_GIT_COMMANDS_DOCS_SLUG: &str = "tasks#custom-git-commands";

pub(crate) struct CommitContextMenuData {
    pub(crate) sha: Oid,
    pub(crate) parents: Vec<Oid>,
    pub(crate) tag_names: Vec<SharedString>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitContextMenuSource {
    GitGraph,
    GitPanel,
}

pub(crate) fn commit_context_menu(
    commit: CommitContextMenuData,
    source: CommitContextMenuSource,
    ref_name: Option<SharedString>,
    focus_handle: FocusHandle,
    repository: Option<WeakEntity<Repository>>,
    workspace: WeakEntity<Workspace>,
    git_graph: Option<WeakEntity<GitGraph>>,
    compare_selection: Option<Oid>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    let sha = commit.sha;
    let parents = commit.parents;
    let sha_short = sha.display_short();
    let sha_string = sha.to_string();
    let repository_snapshot = repository
        .as_ref()
        .and_then(WeakEntity::upgrade)
        .map(|repository| repository.read(cx).snapshot());
    let has_local_branch = repository_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.branch.is_some());
    let is_head = repository_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.head_commit.as_ref())
        .is_some_and(|head| head.sha.as_ref() == sha_string);
    let git_tasks = git_context_menu_tasks(
        git_task_context(&repository, sha, ref_name.as_deref(), cx),
        &workspace,
        cx,
    );
    let remote_link = repository_snapshot.as_ref().and_then(|snapshot| {
        let remote_url = snapshot
            .remote_upstream_url
            .as_deref()
            .or(snapshot.remote_origin_url.as_deref())?;
        let provider_registry = GitHostingProviderRegistry::default_global(cx);
        let (host, parsed) = parse_git_remote_url(provider_registry, remote_url)?;
        let params = BuildCommitPermalinkParams { sha: &sha_string };
        Some((
            host.name().to_string(),
            host.build_commit_permalink(&parsed, params).to_string(),
        ))
    });
    let header = match &ref_name {
        Some(ref_name) => format!("Ref {ref_name}"),
        None => format!("Commit {sha_short}"),
    };

    ContextMenu::build(window, cx, move |context_menu, _, _| {
        context_menu
            .context(focus_handle)
            .header(header)
            .entry("View Diff", Some(OpenCommitView.boxed_clone()), {
                let repository = repository.clone();
                let workspace = workspace.clone();
                move |window, cx| {
                    let Some(repository) = repository.clone() else {
                        return;
                    };
                    CommitView::open(
                        sha.to_string(),
                        repository,
                        workspace.clone(),
                        None,
                        None,
                        window,
                        cx,
                    );
                }
            })
            .entry(
                "Copy SHA",
                Some(CopyCommitSha.boxed_clone()),
                move |_window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(sha.to_string()));
                },
            )
            .when_some(ref_name.clone(), |menu, ref_name| {
                menu.entry("Copy Ref Name", None, move |_window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(ref_name.to_string()));
                })
            })
            .when(ref_name.is_none(), |menu| {
                menu.map(|menu| {
                    let tag_names = commit.tag_names.clone();
                    let copy_tag_label = "Copy Tag";

                    match tag_names.as_slice() {
                        [] => menu.item(
                            ContextMenuEntry::new(copy_tag_label)
                                .action(CopyCommitTag.boxed_clone())
                                .disabled(true),
                        ),
                        [tag_name] => {
                            let tag_name = tag_name.clone();
                            let label = format!("{copy_tag_label}: {tag_name}");
                            menu.entry(
                                label,
                                Some(CopyCommitTag.boxed_clone()),
                                move |_window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        tag_name.to_string(),
                                    ));
                                },
                            )
                        }
                        _ => menu.submenu(copy_tag_label, move |menu, _window, _cx| {
                            let mut menu = menu.fixed_width(COMMIT_TAG_LIST_WIDTH_IN_REMS.into());

                            for tag_name in tag_names.clone() {
                                let tag_name_to_copy = tag_name.clone();
                                menu = menu.entry(tag_name, None, move |_window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        tag_name_to_copy.to_string(),
                                    ));
                                });
                            }
                            menu
                        }),
                    }
                })
            })
            .entry("Create Tag…", None, {
                let repository = repository.clone();
                let workspace = workspace.clone();
                move |window, cx| {
                    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
                        return;
                    };
                    workspace
                        .update(cx, |workspace, cx| {
                            create_tag_at_commit(sha, false, repository, workspace, window, cx);
                        })
                        .log_err();
                }
            })
            .entry("Inspect Details", None, {
                let git_graph = git_graph.clone();
                let repository = repository.clone();
                let workspace = workspace.clone();
                move |window, cx| {
                    if let Some(git_graph) = git_graph.as_ref().and_then(WeakEntity::upgrade) {
                        git_graph.update(cx, |graph, cx| {
                            graph.select_commit_by_sha(sha, cx);
                        });
                    } else if let Some(repository) =
                        repository.as_ref().and_then(WeakEntity::upgrade)
                    {
                        CommitView::open(
                            sha.to_string(),
                            repository.downgrade(),
                            workspace.clone(),
                            None,
                            None,
                            window,
                            cx,
                        );
                    }
                }
            })
            .submenu("Open Changes", {
                let repository_for_head = repository.clone();
                let workspace_for_head = workspace.clone();
                let head_sha = sha.to_string();
                let repository_for_reverse = repository.clone();
                let workspace_for_reverse = workspace.clone();
                let reverse_head_sha = sha.to_string();
                let repository_for_worktree = repository.clone();
                let workspace_for_worktree = workspace.clone();
                move |menu, _window, _cx| {
                    menu.entry("Compare with HEAD", None, move |window, cx| {
                        let Some(repository) =
                            repository_for_head.as_ref().and_then(WeakEntity::upgrade)
                        else {
                            return;
                        };
                        CommitView::open_compare(
                            head_sha.clone(),
                            "HEAD".to_string(),
                            repository.downgrade(),
                            workspace_for_head.clone(),
                            window,
                            cx,
                        );
                    })
                    .entry("Compare HEAD with this Commit", None, move |window, cx| {
                        let Some(repository) = repository_for_reverse
                            .as_ref()
                            .and_then(WeakEntity::upgrade)
                        else {
                            return;
                        };
                        CommitView::open_compare(
                            "HEAD".to_string(),
                            reverse_head_sha.clone(),
                            repository.downgrade(),
                            workspace_for_reverse.clone(),
                            window,
                            cx,
                        );
                    })
                    .entry(
                        "Compare with Working Tree",
                        None,
                        move |window, cx| {
                            let Some(repository) = repository_for_worktree
                                .as_ref()
                                .and_then(WeakEntity::upgrade)
                            else {
                                return;
                            };
                            workspace_for_worktree
                                .update(cx, |workspace, cx| {
                                    ProjectDiff::deploy_at_commit_worktree(
                                        workspace,
                                        repository,
                                        sha.to_string().into(),
                                        window,
                                        cx,
                                    );
                                })
                                .log_err();
                        },
                    )
                }
            })
            .when_some(git_graph.clone(), |menu, git_graph| {
                let menu = menu.entry("Select for Compare", None, {
                    let git_graph = git_graph.clone();
                    move |_window, cx| {
                        git_graph
                            .update(cx, |graph, cx| graph.set_compare_selection(Some(sha), cx))
                            .ok();
                    }
                });
                match compare_selection {
                    Some(compare_sha) if compare_sha != sha => {
                        let repository = repository.clone();
                        let workspace = workspace.clone();
                        menu.entry(
                            format!("Compare with {}", compare_sha.display_short()),
                            None,
                            move |window, cx| {
                                if let Some(repository) =
                                    repository.as_ref().and_then(WeakEntity::upgrade)
                                {
                                    CommitView::open_compare(
                                        compare_sha.to_string(),
                                        sha.to_string(),
                                        repository.downgrade(),
                                        workspace.clone(),
                                        window,
                                        cx,
                                    );
                                }
                                git_graph
                                    .update(cx, |graph, cx| graph.set_compare_selection(None, cx))
                                    .ok();
                            },
                        )
                    }
                    Some(_) => menu.entry("Clear Compare Selection", None, move |_window, cx| {
                        git_graph
                            .update(cx, |graph, cx| graph.set_compare_selection(None, cx))
                            .ok();
                    }),
                    None => menu,
                }
            })
            .separator()
            .header("Git Operations")
            .when(!is_head, |menu| {
                menu.entry("Checkout Commit…", None, {
                    let repository = repository.clone();
                    move |window, cx| {
                        checkout_commit(sha, repository.clone(), window, cx);
                    }
                })
            })
            .when(has_local_branch, |menu| {
                menu.submenu("Reset Current Branch to Here", {
                    let repository_for_soft = repository.clone();
                    let repository_for_mixed = repository.clone();
                    move |menu, _window, _cx| {
                        menu.entry("Soft…", None, move |window, cx| {
                            reset_to_commit(
                                sha,
                                ResetMode::Soft,
                                repository_for_soft.clone(),
                                window,
                                cx,
                            );
                        })
                        .entry("Mixed…", None, move |window, cx| {
                            reset_to_commit(
                                sha,
                                ResetMode::Mixed,
                                repository_for_mixed.clone(),
                                window,
                                cx,
                            );
                        })
                    }
                })
                .entry("Rebase Current Branch onto Here…", None, {
                    let repository = repository.clone();
                    move |window, cx| {
                        rebase_to_commit(sha, repository.clone(), window, cx);
                    }
                })
            })
            .entry("Revert Commit…", None, {
                let repository = repository.clone();
                let workspace = workspace.clone();
                move |window, cx| {
                    revert_commit(
                        sha,
                        parents.clone(),
                        repository.clone(),
                        workspace.clone(),
                        window,
                        cx,
                    );
                }
            })
            .entry("Create Branch…", None, {
                let repository = repository.clone();
                let workspace = workspace.clone();
                move |window, cx| {
                    create_branch_at_commit(sha, repository.clone(), workspace.clone(), window, cx);
                }
            })
            .entry("Create Patch…", None, {
                let repository = repository.clone();
                move |window, cx| {
                    create_patch(sha, repository.clone(), window, cx);
                }
            })
            .entry("Create Worktree…", None, {
                let repository = repository.clone();
                move |window, cx| {
                    let Some(snapshot) = repository
                        .as_ref()
                        .and_then(WeakEntity::upgrade)
                        .map(|repository| repository.read(cx).snapshot())
                    else {
                        return;
                    };
                    window.dispatch_action(
                        Box::new(zed_actions::CreateWorktree {
                            worktree_name: None,
                            branch_target: zed_actions::NewWorktreeBranchTarget::Commit {
                                sha: sha.to_string(),
                                repository_path: snapshot.work_directory_abs_path.to_path_buf(),
                            },
                        }),
                        cx,
                    );
                }
            })
            .when(source == CommitContextMenuSource::GitPanel, |menu| {
                menu.entry("Show in Git Graph", None, move |window, cx| {
                    window.dispatch_action(
                        Box::new(crate::git_graph::OpenAtCommit {
                            sha: sha.to_string(),
                        }),
                        cx,
                    );
                })
            })
            .when_some(remote_link, |menu, (provider, url)| {
                let browse_url = url.clone();
                let share_url = url.clone();
                menu.submenu("Browse", move |menu, _window, _cx| {
                    menu.entry(format!("Open Commit on {provider}"), None, move |_, cx| {
                        cx.open_url(&browse_url);
                    })
                })
                .submenu("Share", move |menu, _window, _cx| {
                    menu.entry("Copy Commit Link", None, move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(share_url.clone()));
                    })
                })
            })
            .map(|mut menu| {
                menu = menu.separator().header("Custom Commands");

                if git_tasks.is_empty() {
                    return menu.item(
                        ContextMenuEntry::new("Learn More")
                            .icon(IconName::ArrowUpRight)
                            .icon_color(Color::Muted)
                            .icon_position(IconPosition::End)
                            .handler(|_window, cx| {
                                let docs_url =
                                    release_channel::docs_url(CUSTOM_GIT_COMMANDS_DOCS_SLUG, cx);
                                cx.open_url(&docs_url);
                            }),
                    );
                }

                for (task_source_kind, resolved_task) in git_tasks {
                    let label = resolved_task.display_label().to_string();
                    let workspace = workspace.clone();
                    menu = menu.entry(label, None, move |window, cx| {
                        workspace
                            .update(cx, |workspace, cx| {
                                workspace.schedule_resolved_task(
                                    task_source_kind.clone(),
                                    resolved_task.clone(),
                                    false,
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                    });
                }

                menu
            })
    })
}

fn checkout_commit(
    sha: Oid,
    repository: Option<WeakEntity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };
    window
        .spawn(cx, async move |cx| {
            repository
                .update(cx, |repository, cx| {
                    repository.checkout_commit(sha.to_string(), cx)
                })
                .await??;
            Ok(())
        })
        .detach_and_prompt_err("Failed to check out commit", window, cx, |error, _, _| {
            Some(error.to_string())
        });
}

fn reset_to_commit(
    sha: Oid,
    mode: ResetMode,
    repository: Option<WeakEntity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };
    let mode_name = match &mode {
        ResetMode::Soft => "Soft",
        ResetMode::Mixed => "Mixed",
    };
    let confirmation = window.prompt(
        PromptLevel::Warning,
        &format!("Reset current branch to {}?", sha.display_short()),
        Some(&format!(
            "This will perform a {mode_name} reset. The working tree files will be preserved."
        )),
        &["Reset", "Cancel"],
        cx,
    );
    window
        .spawn(cx, async move |cx| {
            if confirmation.await != 0 {
                return Ok(());
            }
            repository
                .update(cx, |repository, cx| {
                    repository.reset(sha.to_string(), mode, cx)
                })
                .await??;
            Ok(())
        })
        .detach_and_prompt_err("Failed to reset branch", window, cx, |error, _, _| {
            Some(error.to_string())
        });
}

fn rebase_to_commit(
    sha: Oid,
    repository: Option<WeakEntity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };
    let confirmation = window.prompt(
        PromptLevel::Warning,
        &format!("Rebase current branch onto {}?", sha.display_short()),
        Some("If Git reports conflicts, resolve them and continue or abort from the integrated terminal."),
        &["Rebase", "Cancel"],
        cx,
    );
    window
        .spawn(cx, async move |cx| {
            if confirmation.await != 0 {
                return Ok(());
            }
            repository
                .update(cx, |repository, cx| {
                    repository.rebase_onto(sha.to_string(), cx)
                })
                .await??;
            Ok(())
        })
        .detach_and_prompt_err("Failed to rebase branch", window, cx, |error, _, _| {
            Some(error.to_string())
        });
}

fn revert_commit(
    sha: Oid,
    parents: Vec<Oid>,
    repository: Option<WeakEntity<Repository>>,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };

    let parent_picker = (parents.len() > 1).then(|| {
        let options = parents
            .iter()
            .enumerate()
            .map(|(index, parent)| {
                SharedString::from(format!("Parent {} ({})", index + 1, parent.display_short()))
            })
            .collect();
        crate::picker_prompt::prompt(
            "Choose the mainline parent to revert",
            options,
            workspace,
            window,
            cx,
        )
    });

    window
        .spawn(cx, async move |cx| {
            let mainline_parent = if let Some(parent_picker) = parent_picker {
                let Some(index) = parent_picker.await else {
                    return Ok(());
                };
                Some(index as u32 + 1)
            } else {
                None
            };
            let prompt = cx.update(|window, cx| {
                window.prompt(
                    PromptLevel::Warning,
                    &format!("Revert commit {}?", sha.display_short()),
                    Some("Git will create a new commit that reverses this commit. If conflicts occur, recover from the integrated terminal."),
                    &["Revert", "Cancel"],
                    cx,
                )
            })?;
            if prompt.await != 0 {
                return Ok(());
            }
            repository
                .update(cx, |repository, cx| {
                    repository.revert_commit(sha.to_string(), mainline_parent, cx)
                })
                .await??;
            Ok(())
        })
        .detach_and_prompt_err("Failed to revert commit", window, cx, |error, _, _| {
            Some(error.to_string())
        });
}

fn create_branch_at_commit(
    sha: Oid,
    repository: Option<WeakEntity<Repository>>,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };
    let sha: SharedString = sha.to_string().into();
    workspace
        .update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, move |window, cx| {
                CreateBranchAtCommitModal::new(sha, repository, window, cx)
            });
        })
        .log_err();
}

struct CreateBranchAtCommitModal {
    editor: Entity<Editor>,
    sha: SharedString,
    repository: Entity<Repository>,
}

impl CreateBranchAtCommitModal {
    fn new(
        sha: SharedString,
        repository: Entity<Repository>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(
                format!("commit/{}", sha.get(..7).unwrap_or(sha.as_ref())),
                window,
                cx,
            );
            editor.set_placeholder_text("Branch name", window, cx);
            editor
        });
        Self {
            editor,
            sha,
            repository,
        }
    }

    fn cancel(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let branch_name = self.editor.read(cx).text(cx).trim().to_owned();
        if branch_name.is_empty() {
            cx.emit(DismissEvent);
            return;
        }
        let repository = self.repository.clone();
        let base_commit = self.sha.to_string();
        window
            .spawn(cx, async move |cx| {
                repository
                    .update(cx, |repository, _| {
                        repository.create_branch(branch_name, Some(base_commit))
                    })
                    .await??;
                Ok(())
            })
            .detach_and_prompt_err("Failed to create branch", window, cx, |error, _, _| {
                Some(error.to_string())
            });
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for CreateBranchAtCommitModal {}
impl ModalView for CreateBranchAtCommitModal {}
impl Focusable for CreateBranchAtCommitModal {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for CreateBranchAtCommitModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("CreateBranchAtCommitModal")
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .elevation_2(cx)
            .w(gpui::rems(34.))
            .child(Label::new(format!(
                "Create branch at {}",
                self.sha.display_short()
            )))
            .child(div().px_3().py_2().child(self.editor.clone()))
    }
}

fn create_patch(
    sha: Oid,
    repository: Option<WeakEntity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(repository) = repository.as_ref().and_then(WeakEntity::upgrade) else {
        return;
    };
    let snapshot = repository.read(cx).snapshot();
    let suggested_name = format!("{}.patch", sha.display_short());
    let destination =
        cx.prompt_for_new_path(&snapshot.work_directory_abs_path, Some(&suggested_name));
    window
        .spawn(cx, async move |cx| {
            let Some(path) = destination.await?? else {
                return Ok(());
            };
            let patch = repository
                .update(cx, |repository, _| repository.format_patch(sha.to_string()))
                .await??;
            anyhow::ensure!(!patch.is_empty(), "This commit has no changes to export");
            let fs = cx.update(|_, cx| <dyn Fs>::global(cx))?;
            fs.write(&path, &patch).await?;
            Ok(())
        })
        .detach_and_prompt_err("Failed to create patch", window, cx, |error, _, _| {
            Some(error.to_string())
        });
}

pub(crate) fn git_task_context(
    repository: &Option<WeakEntity<Repository>>,
    commit_sha: git::Oid,
    ref_name: Option<&str>,
    cx: &App,
) -> Option<TaskContext> {
    let repository_path = repository
        .as_ref()?
        .upgrade()?
        .read(cx)
        .work_directory_abs_path
        .to_path_buf();
    let repository_name = repository_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string);
    let mut task_variables = TaskVariables::from_iter([
        (VariableName::GitSha, commit_sha.to_string()),
        (VariableName::GitShaShort, commit_sha.display_short()),
        (
            VariableName::GitRepositoryPath,
            repository_path.to_string_lossy().into_owned(),
        ),
    ]);

    if let Some(repository_name) = repository_name {
        task_variables.insert(VariableName::GitRepositoryName, repository_name);
    }
    if let Some(ref_name) = ref_name {
        task_variables.insert(VariableName::GitRef, ref_name.to_string());
    }

    Some(TaskContext {
        cwd: Some(repository_path),
        task_variables,
        ..TaskContext::default()
    })
}

pub(crate) fn git_context_menu_tasks(
    task_context: Option<TaskContext>,
    workspace: &WeakEntity<Workspace>,
    cx: &App,
) -> Vec<(project::TaskSourceKind, task::ResolvedTask)> {
    let Some(task_context) = task_context else {
        return Vec::new();
    };
    let Some(workspace) = workspace.upgrade() else {
        return Vec::new();
    };
    let project = workspace.read(cx).project().clone();
    let task_inventory = project.read_with(cx, |project, cx| {
        project.task_store().read(cx).task_inventory().cloned()
    });
    let Some(task_inventory) = task_inventory else {
        return Vec::new();
    };

    task_inventory
        .read(cx)
        .resolve_global_tasks_with_tag(GIT_COMMAND_TASK_TAG, &task_context)
}
