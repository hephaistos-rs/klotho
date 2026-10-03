//! `/-/settings/tokens`: personal access tokens (FR-AUTH-011, 012). A new
//! token is shown once, on the page that created it, and never again.

use klotho_core::{Error, Scope, Scopes, TokenInfo};
use serde::Deserialize;
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::content::Form;
use topcoat::router::error::{SeeOther, see_other};
use topcoat::router::{HeaderValue, StatusCode, header, page, path_param, query_params, route};
use topcoat::view::{View, attributes, component, view};

use crate::components::alert::{AlertVariant, alert, alert_description, alert_title};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button, button_variants};
use crate::components::checkbox::checkbox;
use crate::components::field::{
    FieldLegendVariant, field, field_description, field_error, field_label, field_legend, field_set,
};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::session::{core, require_user};
use crate::ui::{
    code_line, empty_state, form_actions, form_error, list, list_row, notice, page_header, section_heading,
};

const TOKENS_PATH: &str = "/-/settings/tokens";

/// `?revoke=<id>` asks to confirm revoking one of the user's tokens. Anything
/// else (an unknown id, someone else's id, not a number) shows the plain page.
/// `?revoked=<id>` is where a successful revoke lands, to say that it worked.
/// It says so only while that id is not one of the user's live tokens.
#[query_params(error = bad_request)]
struct TokensQuery {
    revoke: Option<String>,
    revoked: Option<String>,
}

#[page("/-/settings/tokens")]
pub async fn tokens_page(cx: &Cx) -> Result<impl View> {
    let user = require_user(cx).await?;
    let query = topcoat::router::query_params::<TokensQuery>(cx)?;
    let confirm = query.revoke.as_deref().and_then(|id| id.parse::<i64>().ok());
    let revoked_id = query.revoked.as_deref().and_then(|id| id.parse::<i64>().ok());
    let tokens = core(cx).list_tokens(user.id).await?;
    let revoked = revoked_id.is_some_and(|id| tokens.iter().all(|token| token.id != id));
    let is_admin = user.is_admin;
    Ok(view! {
        ((header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        tokens_view(
            tokens: tokens,
            is_admin: is_admin,
            created: None,
            confirm: confirm,
            revoked: revoked,
            draft: Draft::default()
        )
    })
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
        Scopes::new(checked.into_iter().filter(|(value, _)| value.is_some()).map(|(_, scope)| scope))
    }
}

/// A token just created: shown once, with its secret.
struct Created {
    id: i64,
    name: String,
    secret: String,
}

/// Which part of the create form an error belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ErrorAt {
    Name,
    Scopes,
    Form,
}

/// What the create form shows: empty at first, and after a rejected submit
/// the values that were sent and the error next to the part it is about.
struct Draft {
    name: String,
    scopes: Scopes,
    expires_days: Option<u32>,
    error: Option<(ErrorAt, String)>,
}

impl Default for Draft {
    fn default() -> Self {
        Self { name: String::new(), scopes: Scopes::default(), expires_days: Some(90), error: None }
    }
}

impl Draft {
    fn error_at(&self, at: ErrorAt) -> Option<String> {
        self.error.as_ref().filter(|(place, _)| *place == at).map(|(_, message)| message.clone())
    }
}

/// Where core's rejection of `form` belongs, judged from the form itself in
/// the order core checks it, so core's own message is shown unchanged.
fn error_at(form: &CreateForm, scopes: &Scopes, is_admin: bool) -> ErrorAt {
    let name = form.name.trim();
    if name.is_empty() || name.len() > 100 {
        ErrorAt::Name
    } else if scopes.is_empty() || (scopes.contains(Scope::Admin) && !is_admin) {
        ErrorAt::Scopes
    } else {
        ErrorAt::Form
    }
}

/// Core's messages are lower-case fragments; the page shows sentences.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    let mut out: String = chars.next().map(|first| first.to_uppercase().collect()).unwrap_or_default();
    out.push_str(chars.as_str());
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

#[page(POST "/-/settings/tokens")]
pub async fn create_token(cx: &Cx, Form(form): Form<CreateForm>) -> Result<impl View> {
    let user = require_user(cx).await?;
    let is_admin = user.is_admin;
    let scopes = form.scopes();
    let expires_at = form
        .expires_days
        .map(|days| jiff::Timestamp::now() + jiff::SignedDuration::from_hours(24 * i64::from(days)));
    let result = core(cx).create_token(user, &form.name, scopes.clone(), expires_at).await;
    let (created, draft) = match result {
        Ok(token) => {
            let created = Created { id: token.info.id, name: token.info.name, secret: token.secret };
            (Some(created), Draft::default())
        }
        Err(Error::InvalidInput(message)) => {
            let at = error_at(&form, &scopes, is_admin);
            let draft = Draft {
                name: form.name,
                scopes,
                expires_days: form.expires_days,
                error: Some((at, sentence(&message))),
            };
            (None, draft)
        }
        Err(err) => return Err(err.into()),
    };
    let status = if draft.error.is_some() { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::OK };
    let tokens = core(cx).list_tokens(user.id).await?;
    Ok(view! {
        (status)
        ((header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        tokens_view(
            tokens: tokens,
            is_admin: is_admin,
            created: created,
            confirm: None,
            revoked: false,
            draft: draft
        )
    })
}

path_param!(token_id: i64, error = bad_request);

#[route(POST "/-/settings/tokens/{token_id}/revoke")]
pub async fn revoke_token(cx: &Cx) -> Result<SeeOther> {
    let user = require_user(cx).await?;
    let id = *path_param::<TokenId>(cx)?;
    match core(cx).revoke_token(user.id, id).await {
        Ok(()) => Ok(see_other(format!("{TOKENS_PATH}?revoked={id}"))),
        // Already gone, or never the user's: back to the list, claiming nothing.
        Err(Error::TokenNotFound) => Ok(see_other(TOKENS_PATH)),
        Err(err) => Err(err.into()),
    }
}

/// A date for a `<time>` element: what it shows, and its `datetime` value.
fn day(at: jiff::Timestamp) -> (String, String) {
    (at.strftime("%Y-%m-%d").to_string(), at.to_string())
}

#[component]
async fn tokens_view(
    tokens: Vec<TokenInfo>,
    is_admin: bool,
    created: Option<Created>,
    confirm: Option<i64>,
    revoked: bool,
    draft: Draft,
) -> Result<impl View> {
    let new_id = created.as_ref().map(|created| created.id);
    let created =
        created.map(|created| (format!("Token \u{201c}{}\u{201d} created", created.name), created.secret));
    Ok(view! {
        <div class="max-w-3xl">
            page_header(
                title: "Access tokens",
                description: "Use a token as the password when you push or pull with git, or as a bearer token for the API."
            )
            if let Some((title, secret)) = created {
                notice(
                    variant: AlertVariant::Success,
                    title: &title,
                    <p class="font-medium">
                        "Copy your new token now. It won't be shown again."
                    </p>
                    <div data-token="" class="mt-2 [&_code]:select-all">
                        code_line(value: &secret, wrap: true)
                    </div>
                )
            }

            if revoked {
                notice(variant: AlertVariant::Success, title: "Token revoked.")
            }

            section_heading(title: "Your tokens", id: "tokens-heading")
            if tokens.is_empty() {
                empty_state(
                    title: "You have no tokens yet.",
                    "Create one below to push and pull with git or to call the API."
                )
            } else {
                list(
                    attrs: attributes! { aria-labelledby="tokens-heading" },
                    for token in tokens {
                        let is_new = Some(token.id) == new_id;
                        let confirming = Some(token.id) == confirm;
                        token_row(token: token, is_new: is_new, confirming: confirming)
                    }
                )
            }

            section_heading(title: "New token", id: "new-token-heading")
            create_form(is_admin: is_admin, draft: draft)
        </div>
    })
}

#[component]
async fn create_form(is_admin: bool, draft: Draft) -> Result<impl View> {
    let scopes: Vec<Scope> =
        Scope::ALL.into_iter().filter(|scope| *scope != Scope::Admin || is_admin).collect();
    let name_error = draft.error_at(ErrorAt::Name);
    let scopes_error = draft.error_at(ErrorAt::Scopes);
    let form_level = draft.error_at(ErrorAt::Form);
    let name_invalid = name_error.is_some().then_some("true");
    let name_described =
        if name_error.is_some() { "token-name-hint token-name-error" } else { "token-name-hint" };
    let scopes_invalid = scopes_error.is_some().then_some("true");
    let scopes_described = scopes_error.is_some().then_some("token-scopes-error");
    let expires = draft.expires_days;
    let selected = move |days: Option<u32>| (expires == days).then_some("");
    Ok(view! {
        <form
            method="post"
            action="/-/settings/tokens#new-token-heading"
            aria-labelledby="new-token-heading"
            class="flex max-w-md flex-col gap-4"
        >
            form_error(error: form_level)
            field(
                field_label(attrs: attributes! { for="token-name" }, "Name")
                input(
                    attrs: attributes! {
                        id="token-name"
                        type="text"
                        name="name"
                        value=(draft.name.clone())
                        required=""
                        maxlength="100"
                        autocomplete="off"
                        aria-invalid=(name_invalid)
                        aria-describedby=(name_described)
                    }
                )
                field_description(
                    attrs: attributes! { id="token-name-hint" },
                    "What the token is for, such as the machine it lives on."
                )
                if let Some(error) = name_error {
                    field_error(attrs: attributes! { id="token-name-error" }, (error))
                }
            )
            field(
                field_label(attrs: attributes! { for="token-expires" }, "Expires")
                select(
                    attrs: attributes! { id="token-expires" name="expires_days" },
                    <option value="30" selected=(selected(Some(30)))>
                        "In 30 days"
                    </option>
                    <option value="90" selected=(selected(Some(90)))>
                        "In 90 days"
                    </option>
                    <option value="365" selected=(selected(Some(365)))>
                        "In a year"
                    </option>
                    <option value="" selected=(selected(None))>"Never"</option>
                )
            )
            field_set(
                attrs: attributes! {
                    class="mt-2"
                    aria-describedby=(scopes_described)
                    data-invalid=(scopes_invalid)
                },
                field_legend(variant: FieldLegendVariant::Label, "Scopes")
                if let Some(error) = scopes_error {
                    field_error(attrs: attributes! { id="token-scopes-error" }, (error))
                }
                for scope in scopes {
                    let name = scope.as_str().replace(':', "_");
                    let id = format!("scope-{name}");
                    let description_id = format!("{id}-description");
                    let checked = draft.scopes.contains(scope).then_some("");
                    <div class="flex items-start gap-2">
                        checkbox(
                            attrs: attributes! {
                                id=(id.clone())
                                name=(name)
                                checked=(checked)
                                aria-invalid=(scopes_invalid)
                                aria-describedby=(description_id.clone())
                            }
                        )
                        <div class="min-w-0 pt-0.5">
                            label(
                                attrs: attributes! { for=(id) },
                                <span class="font-mono text-meta">(scope.as_str())</span>
                            )
                            <p
                                id=(description_id)
                                class="text-meta text-muted-foreground"
                            >
                                (scope.description())
                            </p>
                        </div>
                    </div>
                }
            )
            form_actions(button(attrs: attributes! { type="submit" }, "Create token"))
        </form>
    })
}

/// One token in the list: its name, its scopes, its dates, and the revoke
/// link. With `confirming`, the row asks before it revokes. Long names wrap
/// rather than widen the page.
#[component]
async fn token_row(token: TokenInfo, is_new: bool, confirming: bool) -> Result<impl View> {
    let expired = token.expires_at.is_some_and(|at| at <= jiff::Timestamp::now());
    let created = day(token.created_at);
    let expires = token.expires_at.map(day);
    let used = token.last_used_at.map(day);
    let scope_names: Vec<&'static str> = token.scopes.iter().map(|scope| scope.as_str()).collect();
    let row_id = format!("token-{}", token.id);
    let ask = format!("{TOKENS_PATH}?revoke={}#{row_id}", token.id);
    let cancel = format!("{TOKENS_PATH}#{row_id}");
    let revoke = format!("{TOKENS_PATH}/{}/revoke", token.id);
    let name = token.name;
    Ok(view! {
        list_row(
            attrs: attributes! { id=(row_id) },
            <div>
                <div class="flex flex-wrap items-center gap-2">
                    <p class="min-w-0 font-medium wrap-anywhere">(name.clone())</p>
                    if is_new {
                        badge(variant: BadgeVariant::Success, "New")
                    }
                    if expired {
                        badge(variant: BadgeVariant::Destructive, "Expired")
                    }
                </div>
                <p class="sr-only">
                    "Scopes: "
                    (token.scopes.to_string())
                </p>
                <div aria-hidden="true" class="mt-1 flex flex-wrap gap-1">
                    for scope in scope_names {
                        badge(
                            variant: BadgeVariant::Ref,
                            attrs: attributes! { title=(scope) },
                            (scope)
                        )
                    }
                </div>
                <p
                    class="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-meta text-muted-foreground"
                >
                    <span>
                        "Created "
                        <time datetime=(created.1)>(created.0)</time>
                    </span>
                    match expires {
                        Some((shown, iso)) => {
                            <span>
                                if expired {
                                    "Expired "
                                } else {
                                    "Expires "
                                }
                                <time datetime=(iso)>(shown)</time>
                            </span>
                        }
                        None => {
                            <span>"Never expires"</span>
                        }
                    }
                    match used {
                        Some((shown, iso)) => {
                            <span>
                                "Last used "
                                <time datetime=(iso)>(shown)</time>
                            </span>
                        }
                        None => {
                            <span>"Never used"</span>
                        }
                    }
                </p>
                if confirming {
                    alert(
                        variant: AlertVariant::Destructive,
                        attrs: attributes! { class="mt-3" },
                        alert_title(
                            <span class="wrap-anywhere">
                                "Revoke \u{201c}"
                                (name.clone())
                                "\u{201d}?"
                            </span>
                        )
                        alert_description(
                            <p>
                                "Anything using it stops working. This can't be undone."
                            </p>
                            form_actions(
                                <form method="post" action=(revoke)>
                                    button(
                                        variant: ButtonVariant::Destructive,
                                        size: ButtonSize::Sm,
                                        attrs: attributes! { type="submit" },
                                        "Revoke token"
                                    )
                                </form>
                                <a
                                    href=(cancel)
                                    class=(button_variants(
                                        ButtonVariant::Outline,
                                        ButtonSize::Sm,
                                    ))
                                >
                                    "Cancel"
                                </a>
                            )
                        )
                    )
                }
            </div>
            if !confirming {
                <a
                    href=(ask)
                    class=(button_variants(
                        ButtonVariant::DestructiveOutline,
                        ButtonSize::Sm,
                    ))
                >
                    "Revoke"
                    <span class="sr-only">
                        " "
                        (name.clone())
                    </span>
                </a>
            }
        )
    })
}
