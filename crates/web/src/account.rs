//! Signing in, signing up and signing out: plain forms, no JavaScript.
//!
//! Every `POST` here passes Topcoat's origin check first, which rejects
//! cross-site form posts (NFR-SEC-013); the session cookie is `SameSite=Lax`
//! on top of that.

use klotho_core::config::Registration;
use klotho_core::{Error, NewUser};
use serde::Deserialize;
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::content::Form;
use topcoat::router::error::{SeeOther, see_other};
use topcoat::router::{StatusCode, page, query_params, route};
use topcoat::view::{View, attributes, component, view};

use crate::components::button::button;
use crate::session::{core, current_user, safe_return_to, sign_in, sign_out};
use crate::ui::{field, form_actions, form_error, page_header};

#[query_params(error = bad_request)]
struct LoginQuery {
    return_to: Option<String>,
}

#[page("/-/login")]
pub async fn login_page(cx: &Cx) -> Result<impl View> {
    let query = topcoat::router::query_params::<LoginQuery>(cx)?;
    let return_to = safe_return_to(query.return_to.as_deref()).to_owned();
    if current_user(cx).await.is_some() {
        return Err(see_other(&return_to).into());
    }
    let can_register = core(cx).registration() == Registration::Open;
    Ok(view! {
        login_form(
            return_to: return_to,
            login: String::new(),
            error: None,
            can_register: can_register
        )
    })
}

#[derive(Deserialize)]
pub struct LoginForm {
    login: String,
    password: String,
    return_to: Option<String>,
}

#[page(POST "/-/login")]
pub async fn login_submit(cx: &Cx, Form(form): Form<LoginForm>) -> Result<impl View> {
    let return_to = safe_return_to(form.return_to.as_deref()).to_owned();
    match core(cx).sign_in(&form.login, &form.password).await {
        Ok(user) => {
            sign_in(cx, &user).await?;
            Err(see_other(&return_to).into())
        }
        Err(Error::InvalidCredentials) => {
            let can_register = core(cx).registration() == Registration::Open;
            Ok(view! {
                (StatusCode::UNAUTHORIZED)
                login_form(
                    return_to: return_to,
                    login: form.login,
                    error: Some("Incorrect username or password.".to_owned()),
                    can_register: can_register
                )
            })
        }
        Err(err) => Err(err.into()),
    }
}

#[component]
async fn login_form(
    return_to: String,
    login: String,
    error: Option<String>,
    can_register: bool,
) -> Result<impl View> {
    Ok(view! {
        <div class="max-w-md">
            page_header(title: "Sign in")
            <form method="post" action="/-/login" class="flex flex-col gap-4">
                form_error(error: error)
                <input type="hidden" name="return_to" value=(return_to)>
                field(
                    name: "login",
                    label: "Username",
                    kind: "text",
                    value: &login,
                    autocomplete: "username"
                )
                field(
                    name: "password",
                    label: "Password",
                    kind: "password",
                    value: "",
                    autocomplete: "current-password"
                )
                form_actions(button(attrs: attributes! { type="submit" }, "Sign in"))
            </form>
            if can_register {
                <p class="mt-6 text-sm text-muted-foreground">
                    "No account yet? "
                    <a href="/-/register">"Create one"</a>
                </p>
            }
        </div>
    })
}

#[query_params(error = bad_request)]
struct RegisterQuery {
    invite: Option<String>,
}

#[page("/-/register")]
pub async fn register_page(cx: &Cx) -> Result<impl View> {
    let invite = topcoat::router::query_params::<RegisterQuery>(cx)?.invite.clone();
    let mode = core(cx).registration();
    Ok(view! {
        register_form(
            mode: mode,
            invite: invite,
            username: String::new(),
            email: String::new(),
            error: None
        )
    })
}

#[derive(Deserialize)]
pub struct RegisterForm {
    username: String,
    email: String,
    password: String,
    invite: Option<String>,
}

#[page(POST "/-/register")]
pub async fn register_submit(cx: &Cx, Form(form): Form<RegisterForm>) -> Result<impl View> {
    let new = NewUser {
        username: &form.username,
        email: Some(&form.email),
        password: Some(&form.password),
        admin: false,
    };
    let mode = core(cx).registration();
    match core(cx).register(new, form.invite.as_deref()).await {
        Ok(user) => {
            sign_in(cx, &user).await?;
            Err(see_other("/").into())
        }
        Err(
            err @ (Error::InvalidName(_)
            | Error::InvalidInput(_)
            | Error::OwnerExists(_)
            | Error::EmailExists
            | Error::InvalidInvite
            | Error::RegistrationClosed),
        ) => Ok(view! {
            if !matches!(err, Error::RegistrationClosed) {
                (StatusCode::UNPROCESSABLE_ENTITY)
            }
            register_form(
                mode: mode,
                invite: form.invite,
                username: form.username,
                email: form.email,
                error: Some(RegisterError::from(&err))
            )
        }),
        Err(err) => Err(err.into()),
    }
}

/// Which part of the registration form an error is about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ErrorAt {
    Form,
    Username,
    Email,
}

/// A registration error, worded as a sentence and shown next to the field it
/// is about, or above the form when it isn't about one field.
struct RegisterError {
    at: ErrorAt,
    message: String,
}

impl From<&Error> for RegisterError {
    fn from(err: &Error) -> Self {
        let at = match err {
            Error::InvalidName(_) | Error::OwnerExists(_) => ErrorAt::Username,
            Error::EmailExists => ErrorAt::Email,
            _ => ErrorAt::Form,
        };
        Self { at, message: sentence(&err.to_string()) }
    }
}

/// Capitalises a message and ends it with a full stop.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    let mut out: String = chars.next().map(|first| first.to_uppercase().collect()).unwrap_or_default();
    out.push_str(chars.as_str());
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

#[component]
async fn register_form(
    mode: Registration,
    invite: Option<String>,
    username: String,
    email: String,
    error: Option<RegisterError>,
) -> Result<impl View> {
    let open = match mode {
        Registration::Open => true,
        Registration::Invite => invite.is_some(),
        Registration::Disabled => false,
    };
    let closed_message = if mode == Registration::Invite {
        "Registration needs an invite link. Ask an administrator for one."
    } else {
        "Registration is closed. An administrator can create an account for you."
    };
    let error_at =
        |at: ErrorAt| error.as_ref().filter(|error| error.at == at).map(|error| error.message.clone());
    let form_message = error_at(ErrorAt::Form);
    let username_error = error_at(ErrorAt::Username);
    let email_error = error_at(ErrorAt::Email);
    Ok(view! {
        <div class="max-w-md">
            if open {
                page_header(title: "Create an account")
                <form method="post" action="/-/register" class="flex flex-col gap-4">
                    form_error(error: form_message)
                    if let Some(invite) = invite {
                        <input type="hidden" name="invite" value=(invite)>
                    }
                    field(
                        name: "username",
                        label: "Username",
                        kind: "text",
                        value: &username,
                        autocomplete: "username",
                        error: username_error.as_deref()
                    )
                    field(
                        name: "email",
                        label: "Email",
                        kind: "email",
                        value: &email,
                        autocomplete: "email",
                        error: email_error.as_deref()
                    )
                    field(
                        name: "password",
                        label: "Password",
                        kind: "password",
                        value: "",
                        autocomplete: "new-password",
                        hint: "At least 8 characters."
                    )
                    form_actions(
                        button(attrs: attributes! { type="submit" }, "Create account")
                    )
                </form>
            } else {
                (StatusCode::FORBIDDEN)
                page_header(title: "Create an account", description: closed_message)
            }
            <p class="mt-6 text-sm text-muted-foreground">
                "Already have an account? "
                <a href="/-/login">"Sign in"</a>
            </p>
        </div>
    })
}

#[route(POST "/-/logout")]
pub async fn logout(cx: &Cx) -> Result<SeeOther> {
    sign_out(cx).await?;
    Ok(see_other("/"))
}
