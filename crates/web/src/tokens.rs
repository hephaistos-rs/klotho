//! `/-/settings/tokens`: personal access tokens (FR-AUTH-011, 012). A new
//! token is shown once, on the page that created it, and never again.

use klotho_core::{Error, Scope, Scopes, TokenInfo};
use serde::Deserialize;
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::content::Form;
use topcoat::router::error::{SeeOther, see_other};
use topcoat::router::{StatusCode, page, path_param, route};
use topcoat::view::{View, component, view};

use crate::session::{core, require_user};
use crate::ui::{form_error, page_title};

#[page("/-/settings/tokens")]
pub async fn tokens_page(cx: &Cx) -> Result<impl View> {
    let user = require_user(cx).await?;
    let tokens = core(cx).list_tokens(user.id).await?;
    let is_admin = user.is_admin;
    Ok(view! { tokens_view(tokens: tokens, is_admin: is_admin, secret: None, error: None) })
}

/// The create form. Each scope is its own checkbox field, since form decoding
/// takes one value per name.
#[derive(Deserialize)]
pub struct CreateForm {
    name: String,
    /// Days until it expires; empty for never.
    expires_days: Option<u32>,
    repo_read: Option<String>,
    repo_write: Option<String>,
    repo_admin: Option<String>,
    user_read: Option<String>,
    user_write: Option<String>,
    admin: Option<String>,
}

impl CreateForm {
    fn scopes(&self) -> Scopes {
        let checked = [
            (&self.repo_read, Scope::RepoRead),
            (&self.repo_write, Scope::RepoWrite),
            (&self.repo_admin, Scope::RepoAdmin),
            (&self.user_read, Scope::UserRead),
            (&self.user_write, Scope::UserWrite),
            (&self.admin, Scope::Admin),
        ];
        Scopes::new(checked.into_iter().filter(|(field, _)| field.is_some()).map(|(_, scope)| scope))
    }
}

#[page(POST "/-/settings/tokens")]
pub async fn create_token(cx: &Cx, Form(form): Form<CreateForm>) -> Result<impl View> {
    let user = require_user(cx).await?;
    let expires_at = form
        .expires_days
        .map(|days| jiff::Timestamp::now() + jiff::SignedDuration::from_hours(24 * i64::from(days)));
    let created = core(cx).create_token(user, &form.name, form.scopes(), expires_at).await;
    let (secret, error) = match created {
        Ok(token) => (Some(token.secret), None),
        Err(Error::InvalidInput(message)) => (None, Some(message)),
        Err(err) => return Err(err.into()),
    };
    let status = if error.is_some() { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::OK };
    let tokens = core(cx).list_tokens(user.id).await?;
    let is_admin = user.is_admin;
    Ok(view! {
        (status)
        tokens_view(tokens: tokens, is_admin: is_admin, secret: secret, error: error)
    })
}

path_param!(token_id: i64, error = bad_request);

#[route(POST "/-/settings/tokens/{token_id}/revoke")]
pub async fn revoke_token(cx: &Cx) -> Result<SeeOther> {
    let user = require_user(cx).await?;
    let id = *path_param::<TokenId>(cx)?;
    match core(cx).revoke_token(user.id, id).await {
        Ok(()) | Err(Error::TokenNotFound) => Ok(see_other("/-/settings/tokens")),
        Err(err) => Err(err.into()),
    }
}

fn day(at: jiff::Timestamp) -> String {
    at.strftime("%Y-%m-%d").to_string()
}

#[component]
async fn tokens_view(
    tokens: Vec<TokenInfo>,
    is_admin: bool,
    secret: Option<String>,
    error: Option<String>,
) -> Result<impl View> {
    let scopes: Vec<Scope> =
        Scope::ALL.into_iter().filter(|scope| *scope != Scope::Admin || is_admin).collect();
    let rows: Vec<(String, String, String)> = tokens
        .iter()
        .map(|token| {
            let expires =
                token.expires_at.map_or("never expires".to_owned(), |at| format!("expires {}", day(at)));
            let used =
                token.last_used_at.map_or("never used".to_owned(), |at| format!("last used {}", day(at)));
            let details = format!("{} · {expires} · {used}", token.scopes);
            (token.name.clone(), details, format!("/-/settings/tokens/{}/revoke", token.id))
        })
        .collect();
    Ok(view! {
        page_title(title: "Access tokens")
        <p class="mt-2 text-gray-600">
            "Use a token as the password for git over HTTP, or as "
            <code>"Authorization: Bearer"</code>
            " for the API."
        </p>

        if let Some(secret) = secret {
            <div role="status" class="mt-6 rounded border border-green-300 bg-green-50 p-4">
                <p class="font-medium">"Copy your new token now. It won't be shown again."</p>
                <code class="mt-2 block break-all font-mono" data-token="">(secret)</code>
            </div>
        }

        <h2 class="mt-8 text-lg font-semibold">"Your tokens"</h2>
        if rows.is_empty() {
            <p class="mt-2 text-gray-600">"You have no tokens yet."</p>
        } else {
            <ul class="mt-2 divide-y rounded border">
                for (name, details, revoke) in rows {
                    <li class="flex items-center justify-between gap-4 p-3">
                        <div>
                            <p class="font-medium">(name)</p>
                            <p class="text-sm text-gray-600">(details)</p>
                        </div>
                        <form method="post" action=(revoke)>
                            <button type="submit" class="rounded border border-red-300 px-3 py-1 text-sm text-red-700">"Revoke"</button>
                        </form>
                    </li>
                }
            </ul>
        }

        <h2 class="mt-8 text-lg font-semibold">"New token"</h2>
        <form method="post" action="/-/settings/tokens" class="mt-2 max-w-md space-y-4">
            form_error(error: error)
            <label class="block">
                <span class="text-sm font-medium">"Name"</span>
                <input type="text" name="name" required="" maxlength="100" class="mt-1 block w-full rounded border border-gray-300 px-3 py-2">
            </label>
            <fieldset>
                <legend class="text-sm font-medium">"Scopes"</legend>
                for scope in scopes {
                    let name = scope.as_str().replace(':', "_");
                    <label class="mt-1 flex items-center gap-2">
                        <input type="checkbox" name=(name)>
                        <code>(scope.as_str())</code>
                        <span class="text-sm text-gray-600">(scope.description())</span>
                    </label>
                }
            </fieldset>
            <label class="block">
                <span class="text-sm font-medium">"Expires"</span>
                <select name="expires_days" class="mt-1 block rounded border border-gray-300 px-3 py-2">
                    <option value="30">"In 30 days"</option>
                    <option value="90" selected="">"In 90 days"</option>
                    <option value="365">"In a year"</option>
                    <option value="">"Never"</option>
                </select>
            </label>
            <button type="submit" class="rounded bg-gray-900 px-4 py-2 text-white">"Create token"</button>
        </form>
    })
}
