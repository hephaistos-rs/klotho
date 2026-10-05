//! Browsing code: the repository's home, folders, files and raw downloads
//! (FR-UI-001, 002, 003). Every page works without JavaScript.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::Frame;
use klotho_core::browse::{Contents, DirListing, EntryType, FileInfo, MAX_PAGE, RefTarget, TreeEntry};
use klotho_core::markdown::{LinkBase, render};
use klotho_core::{Readme, Repo};
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::icon::iconify::iconify_icon;
use topcoat::icon::{IconData, icon};
use topcoat::router::error::{redirect, see_other};
use topcoat::router::{Body, HeaderValue, header, page, query_params, route};
use topcoat::view::{Unescaped, View, attributes, component, view};

use crate::components::alert::AlertVariant;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::repo::{
    Loaded, Tab, breadcrumbs, load, page_error, ref_chip, ref_path, repo_header, repo_url, spec,
};
use crate::session::core;
use crate::ui::{code_line, empty_state, list, notice, section_heading};

/// `?cursor=` for the next page of a large folder.
#[query_params(error = bad_request)]
struct TreeQuery {
    cursor: Option<String>,
}

/// `?plain=1` shows a Markdown file's source instead of rendering it.
#[query_params(error = bad_request)]
struct BlobQuery {
    plain: Option<String>,
}

/// The repository's front page: the root folder of the default branch and its
/// README, or how to push the first commit.
#[page("/{owner_name}/{repo_name}")]
pub async fn repo_home(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let clone_url = core(cx).urls().clone(&repo.full_name());
    let content = if info.empty {
        None
    } else {
        let target = core(cx).resolve_ref(&repo, None).await?;
        let listing = match core(cx).contents(&repo, Some(&target.commit), "", None, Some(MAX_PAGE)).await? {
            Contents::Dir(listing) => listing,
            _ => DirListing { path: String::new(), entries: Vec::new(), next: None },
        };
        let readme = core(cx).readme_html(&repo, Some(&target.name), "").await?;
        Some((target, listing, readme))
    };
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch.clone(), tab: Tab::Code)
        match content {
            Some((target, listing, readme)) => {
                <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
                    ref_chip(repo: repo.clone(), name: target.name.clone())
                    clone_box(url: clone_url)
                </div>
                tree_list(
                    repo: repo.clone(),
                    rev: target.name.clone(),
                    listing: listing,
                    cursor_base: None
                )
                readme_box(readme: readme)
            }
            None => {
                empty_repo(clone_url: clone_url, branch: branch)
            }
        }
    })
}

/// A folder at a ref: `/{owner}/{repo}/tree/{ref}/{path}`. A file's URL
/// redirects to its `blob` page.
#[page("/{owner_name}/{repo_name}/-/tree/{*spec}")]
pub async fn tree(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let spec = spec(cx);
    let resolved = core(cx).resolve_spec(&repo, &spec).await.map_err(page_error)?;
    let query = topcoat::router::query_params::<TreeQuery>(cx)?;
    let (rev, path) = (resolved.target.name.clone(), resolved.path.clone());
    let contents = core(cx)
        .contents(&repo, Some(&resolved.target.commit), &path, query.cursor.as_deref(), Some(MAX_PAGE))
        .await
        .map_err(page_error)?;
    let listing = match contents {
        Contents::Dir(listing) => listing,
        _ => return Err(see_other(repo_url(&repo, "blob", &ref_path(&rev, &path))).into()),
    };
    let readme = if query.cursor.is_none() {
        core(cx).readme_html(&repo, Some(&rev), &path).await.map_err(page_error)?
    } else {
        None
    };
    let cursor_base = Some(repo_url(&repo, "tree", &ref_path(&rev, &path)));
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch.clone(), tab: Tab::Code)
        <div class="mb-3 flex flex-wrap items-center gap-3">
            ref_chip(repo: repo.clone(), name: rev.clone())
            breadcrumbs(repo: repo.clone(), rev: rev.clone(), path: path)
        </div>
        tree_list(
            repo: repo.clone(),
            rev: rev,
            listing: listing,
            cursor_base: cursor_base
        )
        readme_box(readme: readme)
    })
}

/// One file at a ref: `/{owner}/{repo}/blob/{ref}/{path}`. Text is shown with
/// line numbers (each line can be linked as `#L12`), Markdown is rendered,
/// and binary or very large files offer a download.
#[page("/{owner_name}/{repo_name}/-/blob/{*spec}")]
pub async fn blob(cx: &Cx) -> Result<impl View> {
    let Loaded { repo, info } = load(cx).await?;
    let branch = info.default_branch.clone().unwrap_or_else(|| "main".to_owned());
    let spec = spec(cx);
    let resolved = core(cx).resolve_spec(&repo, &spec).await.map_err(page_error)?;
    let plain = topcoat::router::query_params::<BlobQuery>(cx)?.plain.is_some();
    let (rev, path) = (resolved.target.name.clone(), resolved.path.clone());
    let contents = core(cx)
        .contents(&repo, Some(&resolved.target.commit), &path, None, None)
        .await
        .map_err(page_error)?;
    let shown = match contents {
        Contents::Dir(_) => return Err(see_other(repo_url(&repo, "tree", &ref_path(&rev, &path))).into()),
        Contents::File(file) => Shown::File(file),
        Contents::Symlink(link) => Shown::Text(format!("Symbolic link to {}", link.target)),
        Contents::Submodule(module) => Shown::Text(format!("Submodule at commit {}", module.commit)),
    };
    let base = LinkBase {
        blob: repo_url(&repo, "blob", &rev),
        raw: repo_url(&repo, "raw", &rev),
        dir: path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned(),
    };
    Ok(view! {
        repo_header(repo: repo.clone(), branch: branch.clone(), tab: Tab::Code)
        <div class="mb-3 flex flex-wrap items-center gap-3">
            ref_chip(repo: repo.clone(), name: rev.clone())
            breadcrumbs(repo: repo.clone(), rev: rev.clone(), path: path.clone())
        </div>
        file_view(
            repo: repo,
            target: resolved.target,
            path: path,
            shown: shown,
            base: base,
            plain: plain
        )
    })
}

/// What the file page shows.
enum Shown {
    File(FileInfo),
    /// A symlink or submodule, described in a line.
    Text(String),
}

#[component]
async fn file_view(
    repo: Repo,
    target: RefTarget,
    path: String,
    shown: Shown,
    base: LinkBase,
    plain: bool,
) -> Result<impl View> {
    let rev_path = ref_path(&target.name, &path);
    let raw_url = repo_url(&repo, "raw", &rev_path);
    let history_url = repo_url(&repo, "commits", &rev_path);
    let file_url = repo_url(&repo, "blob", &rev_path);
    let (meta, body) = match shown {
        Shown::File(file) => {
            let lower = file.name.to_ascii_lowercase();
            let markdown = lower.ends_with(".md") || lower.ends_with(".markdown");
            let meta =
                format!("{}{}", size_label(file.size), if file.executable { " · executable" } else { "" });
            let body = match file.text {
                Some(text) if markdown && !plain => FileBody::Html(render(&text, Some(&base)), true),
                Some(text) => FileBody::Lines(text.lines().map(str::to_owned).collect(), markdown),
                None if file.binary => FileBody::Download("This file is binary, so it isn't shown here."),
                None => FileBody::Download("This file is too large to show here."),
            };
            (Some(meta), body)
        }
        Shown::Text(text) => (None, FileBody::Line(text)),
    };
    let toggle = match &body {
        FileBody::Html(_, true) => Some(("Source", format!("{file_url}?plain=1"))),
        FileBody::Lines(_, true) => Some(("Rendered", file_url.clone())),
        _ => None,
    };
    Ok(view! {
        <section
            aria-label="File"
            class="overflow-hidden rounded-lg border border-border bg-card"
        >
            <div
                class="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-2"
            >
                <p class="text-meta text-muted-foreground">
                    if let Some(meta) = meta {
                        (meta)
                    }
                </p>
                <div class="flex flex-wrap items-center gap-2">
                    if let Some((label, href)) = toggle {
                        <a
                            href=(href)
                            class=(button_variants(ButtonVariant::Ghost, ButtonSize::Sm))
                        >
                            (label)
                        </a>
                    }
                    <a
                        href=(history_url)
                        class=(button_variants(ButtonVariant::Ghost, ButtonSize::Sm))
                    >
                        icon(data: iconify_icon!("lucide:history"))
                        "History"
                    </a>
                    <a
                        href=(raw_url)
                        class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))
                    >
                        icon(data: iconify_icon!("lucide:download"))
                        "Raw"
                    </a>
                </div>
            </div>
            match body {
                FileBody::Html(html, _) => {
                    <div class="markdown px-4 py-6 sm:px-8">
                        (Unescaped::new_unchecked(html))
                    </div>
                }
                FileBody::Lines(lines, _) => {
                    code_lines(lines: lines)
                }
                FileBody::Download(why) => {
                    <div class="px-4 py-6">
                        notice(
                            variant: AlertVariant::Neutral,
                            title: why,
                            "Use Raw to download it."
                        )
                    </div>
                }
                FileBody::Line(text) => {
                    <p class="px-4 py-6 font-mono text-meta wrap-anywhere">(text)</p>
                }
            }
        </section>
    })
}

enum FileBody {
    /// Rendered Markdown; `true` when the source view is available too.
    Html(String, bool),
    /// Text, one entry per line; `true` when it is Markdown shown as source.
    Lines(Vec<String>, bool),
    Download(&'static str),
    Line(String),
}

/// A text file with line numbers. Each number is a link to its own line, so
/// a line can be shared as `#L12`. The numbers aren't part of a selection.
#[component]
async fn code_lines(lines: Vec<String>) -> Result<impl View> {
    Ok(view! {
        <div tabindex="0" class="overflow-x-auto" aria-label="File contents">
            <table class="w-full border-collapse font-mono text-meta">
                <tbody>
                    for (index, line) in lines.into_iter().enumerate() {
                        let number = index + 1;
                        let id = format!("L{number}");
                        let href = format!("#L{number}");
                        <tr id=(id) class="target:bg-accent">
                            <td
                                class="w-px py-px pr-3 pl-4 text-right align-top whitespace-nowrap select-none"
                            >
                                <a
                                    href=(href)
                                    class="rounded-sm text-muted-foreground hover:text-foreground"
                                >
                                    (number)
                                </a>
                            </td>
                            <td class="py-px pr-4 whitespace-pre">(line)</td>
                        </tr>
                    }
                </tbody>
            </table>
        </div>
    })
}

/// A file's bytes, for downloading or embedding: `/{owner}/{repo}/raw/{ref}/{path}`.
/// Always `application/octet-stream` with `nosniff` and a sandboxing CSP, so a
/// pushed HTML or SVG file can't run on this origin (NFR-SEC-010). Streamed,
/// so a large file doesn't sit in memory (NFR-PERF-013).
#[route(GET "/{owner_name}/{repo_name}/-/raw/{*spec}")]
pub async fn raw(cx: &Cx) -> Result<([(header::HeaderName, HeaderValue); 4], Body)> {
    let Loaded { repo, .. } = load(cx).await?;
    let spec = spec(cx);
    let resolved = core(cx).resolve_spec(&repo, &spec).await.map_err(page_error)?;
    if resolved.path.is_empty() {
        return Err(redirect(repo_url(&repo, "tree", &resolved.target.name)).into());
    }
    let file =
        core(cx).raw(&repo, Some(&resolved.target.commit), &resolved.path).await.map_err(page_error)?;
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'; frame-ancestors 'self'"),
        ),
        (header::CONTENT_LENGTH, HeaderValue::from(file.size)),
    ];
    Ok((headers, Body::new(Chunks(file.chunks))))
}

/// The raw reader's chunks as a response body.
struct Chunks(klotho_core::browse::RawChunks);

impl http_body::Body for Chunks {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<std::result::Result<Frame<Bytes>, Self::Error>>> {
        self.0
            .poll_recv(cx)
            .map(|chunk| chunk.map(|chunk| chunk.map(|bytes| Frame::data(Bytes::from(bytes)))))
    }
}

/// The clone URL, ready to copy.
#[component]
async fn clone_box(url: String) -> Result<impl View> {
    Ok(view! {
        <div class="flex min-w-0 items-center gap-2 sm:max-w-md sm:flex-1">
            <span class="shrink-0 text-meta text-muted-foreground">"Clone"</span>
            <div class="min-w-0 flex-1">code_line(value: &url)</div>
        </div>
    })
}

/// A folder's entries: folders first, then files, each linking to its page.
/// `cursor_base` is the page URL for "More files" when the folder is longer
/// than a page.
#[component]
async fn tree_list(
    repo: Repo,
    rev: String,
    listing: DirListing,
    cursor_base: Option<String>,
) -> Result<impl View> {
    let more = match (listing.next, cursor_base) {
        (Some(next), Some(base)) => Some(format!("{base}?cursor={}", crate::session::encode(&next))),
        (Some(_), None) => Some(repo_url(&repo, "tree", &rev)),
        _ => None,
    };
    let empty = listing.entries.is_empty();
    Ok(view! {
        if empty {
            empty_state(title: "This folder is empty.")
        } else {
            list(
                attrs: attributes! {
                    aria-label="Files"
                    class="divide-y-0 [&>li+li]:border-t [&>li+li]:border-border"
                },
                for entry in listing.entries {
                    tree_row(repo: repo.clone(), rev: rev.clone(), entry: entry)
                }
            )
        }
        if let Some(more) = more {
            <p class="mt-3"><a href=(more)>"More files"</a></p>
        }
    })
}

#[component]
async fn tree_row(repo: Repo, rev: String, entry: TreeEntry) -> Result<impl View> {
    let (icon_data, href, label): (IconData, Option<String>, &str) = match entry.kind {
        EntryType::Dir => (
            iconify_icon!("lucide:folder"),
            Some(repo_url(&repo, "tree", &ref_path(&rev, &entry.path))),
            "Folder",
        ),
        EntryType::File | EntryType::Executable => (
            iconify_icon!("lucide:file"),
            Some(repo_url(&repo, "blob", &ref_path(&rev, &entry.path))),
            "File",
        ),
        EntryType::Symlink => (
            iconify_icon!("lucide:file-symlink"),
            Some(repo_url(&repo, "blob", &ref_path(&rev, &entry.path))),
            "Link",
        ),
        EntryType::Submodule => (iconify_icon!("lucide:folder-git-2"), None, "Submodule"),
    };
    let size = entry.size.map(size_label);
    Ok(view! {
        // Not a `list_row`: a file's size stays on its line at every width.
        <li class="flex items-center justify-between gap-4 px-4 py-2">
            <div class="flex min-w-0 items-center gap-3">
                <span
                    class="shrink-0 text-muted-foreground [&_svg]:size-4"
                    title=(label)
                >
                    icon(data: icon_data)
                    <span class="sr-only">(label)</span>
                </span>
                match href {
                    Some(href) => {
                        <a
                            href=(href)
                            class="min-w-0 rounded-sm text-sm font-medium wrap-anywhere hover:underline"
                        >
                            (entry.name)
                        </a>
                    }
                    None => {
                        <span class="min-w-0 text-sm font-medium wrap-anywhere">
                            (entry.name)
                        </span>
                    }
                }
            </div>
            if let Some(size) = size {
                <span class="shrink-0 text-meta text-muted-foreground tabular-nums">
                    (size)
                </span>
            }
        </li>
    })
}

/// The README below a folder listing, rendered.
#[component]
async fn readme_box(readme: Option<Readme>) -> Result<impl View> {
    Ok(view! {
        if let Some(readme) = readme {
            section_heading(title: &readme.name, id: "readme-heading")
            <article
                aria-labelledby="readme-heading"
                class="markdown rounded-lg border border-border bg-card px-4 py-6 sm:px-8"
            >
                (Unescaped::new_unchecked(readme.html))
            </article>
        }
    })
}

/// An empty repository: how to push the first commit.
#[component]
async fn empty_repo(clone_url: String, branch: String) -> Result<impl View> {
    let new = format!(
        "git init\ngit add .\ngit commit -m \"First commit\"\ngit branch -M {branch}\ngit remote add origin {clone_url}\ngit push -u origin {branch}"
    );
    let existing = format!("git remote add origin {clone_url}\ngit push -u origin {branch}");
    Ok(view! {
        empty_state(
            title: "This repository is empty.",
            "Push a first commit to it with git, using an "
            <a href="/-/settings/tokens">"access token"</a>
            " as the password."
        )
        section_heading(title: "Start a new repository")
        <pre class="rounded-md border border-border bg-card px-3 py-2 text-meta">
            (new)
        </pre>
        section_heading(title: "Push an existing repository")
        <pre class="rounded-md border border-border bg-card px-3 py-2 text-meta">
            (existing)
        </pre>
    })
}

/// A size in bytes for people: `512 B`, `1.4 KB`, `3.0 MB`.
pub fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
