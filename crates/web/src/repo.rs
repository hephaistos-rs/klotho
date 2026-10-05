//! What every repository page shares: the path parameters, loading the
//! repository for the signed-in reader (through `Core::repo_for`, so
//! "not visible" is the same 404 as "doesn't exist", FR-ACL-013), the header
//! with its tabs, and the URLs of the pages (FR-UI-001–007).

use klotho_core::browse::{GitTime, RepoInfo};
use klotho_core::{Action, Actor, Error, Repo, encode_path};
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::error::{bad_request, not_found};
use topcoat::router::path_param;
use topcoat::view::{View, attributes, component, view};

use crate::components::badge::{BadgeVariant, badge};
use crate::session::{core, current_user};

path_param!(pub owner_name);
path_param!(pub repo_name);
path_param!(pub *spec);

/// The reader, as `authorize()` sees them.
pub async fn actor(cx: &Cx) -> Actor {
    current_user(cx).await.cloned().map_or(Actor::Anonymous, Actor::Session)
}

/// A core error, with "nothing here" turned into the page's 404 and a bad
/// cursor into a 400, as the API answers them.
pub fn page_error(err: Error) -> topcoat::Error {
    if err.is_not_found() {
        not_found().into()
    } else if err.is_invalid_cursor() {
        bad_request(err.to_string()).into()
    } else {
        err.into()
    }
}

/// The repository in the URL, if the reader may see it, and its git facts.
pub struct Loaded {
    pub repo: Repo,
    pub info: RepoInfo,
}

pub async fn load(cx: &Cx) -> Result<Loaded> {
    let owner = path_param::<OwnerName>(cx);
    let name = path_param::<RepoName>(cx);
    let actor = actor(cx).await;
    let repo = core(cx).repo_for(&actor, owner, name, Action::Read).await.map_err(page_error)?;
    let info = core(cx).repo_info(&repo).await?;
    Ok(Loaded { repo, info })
}

/// The `ref/path` after `tree/`, `blob/`, `raw/` or `commits/`, decoded.
pub fn spec(cx: &Cx) -> String {
    path_param::<Spec>(cx).collect::<Vec<_>>().join("/")
}

/// The URL of a repository page: `/{owner}/{repo}/-/{kind}/{rest}`, or the
/// repository's home with an empty `kind`.
pub fn repo_url(repo: &Repo, kind: &str, rest: &str) -> String {
    let base = format!("/{}/{}", encode_path(&repo.owner.name), encode_path(&repo.name));
    match (kind, rest.trim_matches('/')) {
        ("", _) => base,
        (kind, "") => format!("{base}/-/{kind}"),
        (kind, rest) => format!("{base}/-/{kind}/{}", encode_path(rest)),
    }
}

/// `ref/path` joined for a URL, the path left out when empty.
pub fn ref_path(rev: &str, path: &str) -> String {
    if path.is_empty() { rev.to_owned() } else { format!("{rev}/{path}") }
}

/// The first 7 characters of a commit ID, as git shows it.
pub fn short_id(id: &str) -> String {
    id.chars().take(7).collect()
}

/// A git date for people ("3 Oct 2026") and for machines (RFC 3339).
pub fn dates(time: &GitTime) -> (String, String) {
    let zoned = time.timestamp.to_zoned(jiff::tz::TimeZone::fixed(time.offset));
    (zoned.strftime("%-d %b %Y").to_string(), zoned.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

/// Which tab of the repository header is current.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Code,
    Commits,
    Branches,
    Tags,
}

/// The top of every repository page: `owner / name` as the page title, a
/// Private badge, and the tabs. The current tab carries the thread marker.
#[component]
pub async fn repo_header(repo: Repo, branch: String, tab: Tab) -> Result<impl View> {
    let owner_url = format!("/{}", encode_path(&repo.owner.name));
    let home = repo_url(&repo, "", "");
    let tabs = [
        (Tab::Code, "Code", home.clone()),
        (Tab::Commits, "Commits", repo_url(&repo, "commits", &branch)),
        (Tab::Branches, "Branches", repo_url(&repo, "branches", "")),
        (Tab::Tags, "Tags", repo_url(&repo, "tags", "")),
    ];
    Ok(view! {
        <div class="mb-6">
            <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
                <h1
                    class="font-display text-3xl leading-tight font-semibold wrap-anywhere sm:text-4xl"
                >
                    <a href=(owner_url) class="rounded-sm hover:underline">
                        (repo.owner.name.clone())
                    </a>
                    <span class="text-muted-foreground">" / "</span>
                    <a href=(home) class="rounded-sm hover:underline">
                        (repo.name.clone())
                    </a>
                </h1>
                if repo.private {
                    badge(variant: BadgeVariant::Secondary, "Private")
                }
            </div>
            <nav
                aria-label="Repository"
                class="mt-4 flex gap-1 overflow-x-auto border-b border-border"
            >
                for (this, label, href) in tabs {
                    let here = (this == tab).then_some("page");
                    <a
                        href=(href)
                        aria-current=(here)
                        class="-mb-px inline-flex h-10 shrink-0 items-center border-b-2 border-transparent px-3 \
                               text-sm font-medium text-muted-foreground transition-colors duration-150 \
                               hover:text-foreground aria-[current=page]:border-thread-ink \
                               aria-[current=page]:text-foreground"
                    >
                        (label)
                    </a>
                }
            </nav>
        </div>
    })
}

/// Where the reader is inside a tree: the repository root, then each folder,
/// each one a link but the last.
#[component]
pub async fn breadcrumbs(repo: Repo, rev: String, path: String) -> Result<impl View> {
    let mut crumbs = vec![(repo.name.clone(), repo_url(&repo, "tree", &rev))];
    let mut sofar = String::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        if !sofar.is_empty() {
            sofar.push('/');
        }
        sofar.push_str(part);
        crumbs.push((part.to_owned(), repo_url(&repo, "tree", &ref_path(&rev, &sofar))));
    }
    let last = crumbs.len() - 1;
    Ok(view! {
        <nav aria-label="Path" class="min-w-0 font-mono text-sm wrap-anywhere">
            for (index, (name, href)) in crumbs.into_iter().enumerate() {
                if index > 0 {
                    <span class="px-1 text-muted-foreground" aria-hidden="true">
                        "/"
                    </span>
                }
                if index == last {
                    <span aria-current="page" class="font-medium">(name)</span>
                } else {
                    <a href=(href) class="rounded-sm text-primary hover:underline">
                        (name)
                    </a>
                }
            }
        </nav>
    })
}

/// The ref being shown, as a chip that leads to the list of branches.
#[component]
pub async fn ref_chip(repo: Repo, name: String) -> Result<impl View> {
    Ok(view! {
        <a
            href=(repo_url(&repo, "branches", ""))
            class="inline-flex max-w-full rounded-md"
            title="Switch branch or tag"
        >
            badge(
                variant: BadgeVariant::Ref,
                attrs: attributes! { title=(name.clone()) },
                (name)
            )
        </a>
    })
}
