---
name: topcoat-ui
description: How to write Klotho's Topcoat web pages, forms and components in crates/web, and the Topcoat quirks that cost compile rounds. Use before adding or changing anything in crates/web.
user-invocable: false
---

# Writing Topcoat pages in Klotho

Klotho's UI is server-rendered with Topcoat ([ADR 0002](../../../docs/decisions/0002-topcoat-ui.md)). Pages must work with JavaScript off. Reference code: `crates/web/src/account.rs` (forms), `tokens.rs` (path params, checkboxes), `ui.rs` (shared components), `session.rs` (the current user).

## Shape of a page

A page is thin: get the user, call one or two `klotho-core` services, render. If you need a query no service offers, add the service (and its API endpoint) in `klotho-core` rather than querying from the page.

```rust
#[page("/{owner}/{repo}")]
pub async fn repo_home(cx: &Cx) -> Result<impl View> {
    let user = current_user(cx).await;          // Option<&User>, memoized per request
    let repo = core(cx).repo_for(&actor, owner, name, Action::Read).await?;
    Ok(view! { repo_view(repo: repo) })          // owned props
}
```

Register every page and route by hand in `service()` in `crates/web/src/lib.rs` (`.page(module::fn)` / `.route(module::fn)`). Pages and their form structs must be `pub`.

## Quirks

- **`view!` only works inside a `#[component]` or `#[page]` function.** Elsewhere use `view! { cx => … }`. Prefer making a component.
- **Views render after the handler returns**, so values moved into a view must be owned. Components called from a page take owned props (`String`, `Option<String>`, owned structs). Small shared components in `ui.rs` take `&str` and are fine when called with literals or values owned by the enclosing component.
- **Components are called with named arguments:** `field(name: "login", label: "Username", kind: "text", value: &login, autocomplete: "username")`.
- **Status codes** go inside the view as an expression: `(StatusCode::UNPROCESSABLE_ENTITY)`. Conditional: `if cond { (StatusCode::FORBIDDEN) }`.
- **Redirect from a `#[page]`** by returning `Err(see_other(&url).into())`. A `#[route]` can return `Result<SeeOther>` and `Ok(see_other("/"))`.
- **Query parameters:** `#[query_params(error = bad_request)] struct Q { … }`, then `topcoat::router::query_params::<Q>(cx)?`. Without `error = bad_request` you get a "borrowed data escapes" error.
- **Path parameters:** `path_param!(token_id: i64, error = bad_request);` then `*path_param::<TokenId>(cx)?`.
- **Forms:** `Form(form): Form<MyForm>` with `#[derive(Deserialize)]`. Form decoding does **not** support repeated keys, so a group of checkboxes is one field per box (`scope_repo_read`, …), each `Option<String>`.
- **Origin check:** Topcoat rejects cross-site state-changing requests with 403 (NFR-SEC-013). Keep every state change a `POST` from a form on our own pages. Never turn the check off.
- **Text is escaped** when interpolated as `(value)`. Rendered Markdown must go through comrak and then `ammonia` before being inserted as raw HTML (NFR-SEC-011).
- **Assets:** without an asset bundle (tests), the layout skips the stylesheet and pages render unstyled. Don't make a page depend on the bundle.
- **Not visible = doesn't exist.** When `repo_for` returns `RepoNotFound`, render the same 404 page as for a missing repository.
- Long `view!` blocks: split into components early, and run `topcoat fmt`.

## Tests

Web pages are tested from `crates/server/tests/` through the full app (`common::state()`), signing in with a form post and keeping the cookie. Check status codes and that key text is in the body; check that private data never appears for strangers.
