//! History: the commit log, branches and tags (FR-UI-005, 007). The log is
//! drawn on the thread, one bead per commit (DESIGN.md P2).

use klotho_core::browse::{BranchInfo, CommitInfo, TagInfo};
use klotho_core::{LogQuery, Repo};
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::error::{SeeOther, see_other};
use topcoat::router::{page, query_params, route};
use topcoat::view::{View, attributes, component, view};

use crate::components::badge::{BadgeVariant, badge};
use crate::repo::{Loaded, Tab, dates, load, page_error, ref_chip, repo_header, repo_url, short_id, spec};
use crate::session::{core, encode};
use crate::ui::{empty_state, list, list_row};

/// `?cursor=` continues a list from the previous page's last item.
#[query_params(error = bad_request)]
struct PageQuery {
    cursor: Option<String>,
}

/// `-/commits` on its own is the default branch's history.
#[route(GET "/{owner_name}/{repo_name}/-/commits")]
pub async fn commits_default(cx: &Cx) -> Result<SeeOther> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.unwrap_or_else(|| "main".to_owned());
    Ok(see_other(repo_url(&repo, "commits", &branch)))
}

/// The history of a ref, or of one path in it: `/{owner}/{repo}/-/commits/{ref}/{path}`.
/// The Commits tab opens it at the default branch; in an empty repository
/// that branch doesn't exist yet, and the log is simply empty.
#[page("/{owner_name}/{repo_name}/-/commits/{*spec}")]
pub async fn commits(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let spec = spec(cx);
    let cursor = topcoat::router::query_params::<PageQuery>(cx)?.cursor.clone();
    let base = repo_url(&repo, "commits", &spec);
    let (rev, path, page) = if info.empty && spec == branch {
        (branch.clone(), String::new(), None)
    } else {
        let resolved = core(cx).resolve_spec(&repo, &spec).await.map_err(page_error)?;
        let log = LogQuery {
            rev: Some(resolved.target.commit.clone()),
            path: resolved.path.clone(),
            cursor,
            limit: None,
        };
        let page = core(cx).commits(&repo, log).await.map_err(page_error)?;
        (resolved.target.name, resolved.path, Some(page))
    };
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch, tab: Tab::Commits)
        commit_log(repo: repo, rev: rev, path: path, page: page, base: base)
    })
}

#[component]
async fn commit_log(
    repo: Repo,
    rev: String,
    path: String,
    page: Option<klotho_core::browse::Page<CommitInfo>>,
    base: String,
) -> Result<impl View> {
    let (items, next) = page.map_or((Vec::new(), None), |page| (page.items, page.next));
    let older = next.map(|next| format!("{base}?cursor={}", encode(&next)));
    let empty = items.is_empty();
    Ok(view! {
        <div class="mb-4 flex flex-wrap items-center gap-3">
            if !rev.is_empty() {
                ref_chip(repo: repo.clone(), name: rev.clone())
            }
            if !path.is_empty() {
                <p class="text-sm text-muted-foreground">
                    "Commits that changed "
                    <code
                        class="font-mono text-meta font-medium text-foreground wrap-anywhere"
                    >
                        (path.clone())
                    </code>
                </p>
            }
        </div>
        if empty {
            empty_state(
                title: "No commits yet.",
                "Push to this repository to start its history."
            )
        } else {
            <div class="relative">
                // The thread runs behind the beads, top bead to bottom bead.
                <div
                    aria-hidden="true"
                    class="absolute top-5 bottom-5 left-1 border-l-2 border-thread-ink"
                ></div>
                <ol aria-label="Commits" class="relative">
                    for commit in items {
                        commit_row(repo: repo.clone(), commit: commit)
                    }
                </ol>
            </div>
        }
        if let Some(older) = older {
            <p class="mt-4"><a href=(older)>"Older commits"</a></p>
        }
    })
}

#[component]
async fn commit_row(repo: Repo, commit: CommitInfo) -> Result<impl View> {
    let (date, datetime) = dates(&commit.committer.date);
    let tree = repo_url(&repo, "tree", &commit.id);
    let short = short_id(&commit.id);
    let merge = commit.parents.len() > 1;
    Ok(view! {
        <li class="flex gap-4 py-3">
            <span
                aria-hidden="true"
                class="mt-1.5 size-2.5 shrink-0 rounded-full border-2 border-thread-ink bg-background"
            ></span>
            <div
                class="flex min-w-0 flex-1 flex-col gap-1 sm:flex-row sm:items-start sm:justify-between sm:gap-4"
            >
                <div class="min-w-0">
                    <a
                        href=(tree.clone())
                        class="rounded-sm font-medium wrap-anywhere hover:underline"
                    >
                        (commit.summary)
                    </a>
                    <p class="text-meta text-muted-foreground">
                        (commit.author.name)
                        " committed "
                        <time datetime=(datetime)>(date)</time>
                        if merge {
                            " · merge"
                        }
                    </p>
                </div>
                <a
                    href=(tree)
                    class="shrink-0 self-start rounded-md"
                    title="Browse the files at this commit"
                >
                    badge(variant: BadgeVariant::Ref, (short))
                </a>
            </div>
        </li>
    })
}

/// Every branch, the default first in its place by name, with its latest commit.
#[page("/{owner_name}/{repo_name}/-/branches")]
pub async fn branches(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let cursor = topcoat::router::query_params::<PageQuery>(cx)?.cursor.clone();
    let page = core(cx).branches(&repo, cursor.as_deref(), None).await.map_err(page_error)?;
    let more = page.next.map(|next| format!("{}?cursor={}", repo_url(&repo, "branches", ""), encode(&next)));
    let empty = page.items.is_empty();
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch.clone(), tab: Tab::Branches)
        if empty {
            empty_state(title: "No branches yet.", "A push creates the first one.")
        } else {
            list(
                attrs: attributes! { aria-label="Branches" },
                for branch in page.items {
                    branch_row(repo: repo.clone(), branch: branch)
                }
            )
        }
        if let Some(more) = more {
            <p class="mt-4"><a href=(more)>"More branches"</a></p>
        }
    })
}

#[component]
async fn branch_row(repo: Repo, branch: BranchInfo) -> Result<impl View> {
    let (date, datetime) = dates(&branch.commit.committer.date);
    Ok(view! {
        list_row(
            <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                    <a
                        href=(repo_url(&repo, "tree", &branch.name))
                        class="rounded-sm font-mono text-sm font-medium wrap-anywhere hover:underline"
                    >
                        (branch.name.clone())
                    </a>
                    if branch.default {
                        badge(variant: BadgeVariant::Secondary, "Default")
                    }
                </div>
                <p class="mt-1 text-meta text-muted-foreground wrap-anywhere">
                    (branch.commit.summary)
                    " · "
                    <time datetime=(datetime)>(date)</time>
                </p>
            </div>
            <a
                href=(repo_url(&repo, "commits", &branch.name))
                class="shrink-0 rounded-sm text-sm font-medium text-primary hover:underline"
            >
                "History"
            </a>
        )
    })
}

/// Every tag, with the commit it comes to.
#[page("/{owner_name}/{repo_name}/-/tags")]
pub async fn tags(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let cursor = topcoat::router::query_params::<PageQuery>(cx)?.cursor.clone();
    let page = core(cx).tags(&repo, cursor.as_deref(), None).await.map_err(page_error)?;
    let more = page.next.map(|next| format!("{}?cursor={}", repo_url(&repo, "tags", ""), encode(&next)));
    let empty = page.items.is_empty();
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch.clone(), tab: Tab::Tags)
        if empty {
            empty_state(title: "No tags yet.", "Push a tag with git push --tags.")
        } else {
            list(
                attrs: attributes! { aria-label="Tags" },
                for tag in page.items {
                    tag_row(repo: repo.clone(), tag: tag)
                }
            )
        }
        if let Some(more) = more {
            <p class="mt-4"><a href=(more)>"More tags"</a></p>
        }
    })
}

#[component]
async fn tag_row(repo: Repo, tag: TagInfo) -> Result<impl View> {
    let commit = tag.commit.map(|commit| {
        let (date, datetime) = dates(&commit.committer.date);
        (commit.summary, date, datetime)
    });
    let message =
        tag.annotation.map(|annotation| annotation.message.trim().to_owned()).filter(|m| !m.is_empty());
    Ok(view! {
        list_row(
            <div class="min-w-0">
                <a
                    href=(repo_url(&repo, "tree", &tag.name))
                    class="rounded-sm font-mono text-sm font-medium wrap-anywhere hover:underline"
                >
                    (tag.name.clone())
                </a>
                if let Some(message) = message {
                    <p class="mt-1 text-sm wrap-anywhere">(message)</p>
                }
                if let Some((summary, date, datetime)) = commit {
                    <p class="mt-1 text-meta text-muted-foreground wrap-anywhere">
                        (summary)
                        " · "
                        <time datetime=(datetime)>(date)</time>
                    </p>
                }
            </div>
            <a
                href=(repo_url(&repo, "commits", &tag.name))
                class="shrink-0 rounded-sm text-sm font-medium text-primary hover:underline"
            >
                "History"
            </a>
        )
    })
}
