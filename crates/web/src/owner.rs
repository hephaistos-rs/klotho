//! An owner's page, `/{owner}`: the repositories the reader can see (FR-UI-023,
//! FR-ACL-011). Private repositories the reader can't open aren't listed at all.

use klotho_core::Repo;
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::{page, path_param, query_params};
use topcoat::view::{View, attributes, component, view};

use crate::components::badge::{BadgeVariant, badge};
use crate::repo::{OwnerName, actor, page_error, repo_url};
use crate::session::{core, encode};
use crate::ui::{empty_state, list, list_row, page_header};

/// Repositories per page.
const PAGE: u32 = 50;

#[query_params(error = bad_request)]
struct OwnerQuery {
    /// The last repository of the previous page.
    cursor: Option<String>,
}

#[page("/{owner_name}")]
pub async fn owner_page(cx: &Cx) -> Result<impl View> {
    let name = path_param::<OwnerName>(cx);
    let owner = core(cx).find_owner(name).await.map_err(page_error)?;
    let cursor = topcoat::router::query_params::<OwnerQuery>(cx)?.cursor.clone();
    let actor = actor(cx).await;
    let repos =
        core(cx).list_repos(&actor, &owner.name, cursor.as_deref(), PAGE).await.map_err(page_error)?;
    let more = (repos.len() == PAGE as usize)
        .then(|| {
            repos
                .last()
                .map(|last| format!("/{}?cursor={}", owner.name, encode(&last.name.to_ascii_lowercase())))
        })
        .flatten();
    Ok(view! {
        page_header(title: &owner.name)
        repo_list(repos: repos, empty: "No repositories here yet.")
        if let Some(more) = more {
            <p class="mt-4"><a href=(more)>"More repositories"</a></p>
        }
    })
}

/// Repositories as a list: the name, linking to the repository, and a
/// Private badge where it applies.
#[component]
pub async fn repo_list(repos: Vec<Repo>, empty: &str) -> Result<impl View> {
    let is_empty = repos.is_empty();
    Ok(view! {
        if is_empty {
            empty_state(title: empty)
        } else {
            list(
                attrs: attributes! { aria-label="Repositories" },
                for repo in repos {
                    let href = repo_url(&repo, "", "");
                    list_row(
                        <div class="flex min-w-0 flex-wrap items-center gap-2">
                            <a
                                href=(href)
                                class="rounded-sm font-medium wrap-anywhere hover:underline"
                            >
                                (repo.full_name())
                            </a>
                            if repo.private {
                                badge(variant: BadgeVariant::Secondary, "Private")
                            }
                        </div>
                    )
                }
            )
        }
    })
}
