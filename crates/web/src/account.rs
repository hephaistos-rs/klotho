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
use topcoat::view::{View, component, view};

use crate::session::{core, current_user, safe_return_to, sign_in, sign_out};
use crate::ui::{field, form_error, page_title};

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
    Ok(view! { login_form(return_to: return_to, login: String::new(), error: None) })
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
        Err(Error::InvalidCredentials) => Ok(view! {
            (StatusCode::UNAUTHORIZED)
            login_form(
                return_to: return_to,
                login: form.login,
                error: Some("Incorrect username or password.".to_owned()),
            )
        }),
        Err(err) => Err(err.into()),
    }
}

#[component]
async fn login_form(return_to: String, login: String, error: Option<String>) -> Result<impl View> {
    Ok(view! {
        page_title(title: "Sign in")
        <form method="post" action="/-/login" class="mt-6 max-w-sm space-y-4">
            form_error(error: error)
            <input type="hidden" name="return_to" value=(return_to)>
            field(name: "login", label: "Username", kind: "text", value: &login, autocomplete: "username")
            field(name: "password", label: "Password", kind: "password", value: "", autocomplete: "current-password")
            <button type="submit" class="rounded bg-gray-900 px-4 py-2 text-white">"Sign in"</button>
        </form>
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
        register_form(mode: mode, invite: invite, username: String::new(), email: String::new(), error: None)
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
            if !matches!(err, Error::RegistrationClosed) { (StatusCode::UNPROCESSABLE_ENTITY) }
            register_form(
                mode: mode,
                invite: form.invite,
                username: form.username,
                email: form.email,
                error: Some(err.to_string()),
            )
        }),
        Err(err) => Err(err.into()),
    }
}

#[component]
async fn register_form(
    mode: Registration,
    invite: Option<String>,
    username: String,
    email: String,
    error: Option<String>,
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
    Ok(view! {
        page_title(title: "Create an account")
        if open {
            <form method="post" action="/-/register" class="mt-6 max-w-sm space-y-4">
                form_error(error: error)
                if let Some(invite) = invite {
                    <input type="hidden" name="invite" value=(invite)>
                }
                field(name: "username", label: "Username", kind: "text", value: &username, autocomplete: "username")
                field(name: "email", label: "Email", kind: "email", value: &email, autocomplete: "email")
                field(
                    name: "password",
                    label: "Password (at least 8 characters)",
                    kind: "password",
                    value: "",
                    autocomplete: "new-password",
                )
                <button type="submit" class="rounded bg-gray-900 px-4 py-2 text-white">"Create account"</button>
            </form>
        } else {
            (StatusCode::FORBIDDEN)
            <p class="mt-4 text-gray-600">(closed_message)</p>
        }
    })
}

#[route(POST "/-/logout")]
pub async fn logout(cx: &Cx) -> Result<SeeOther> {
    sign_out(cx).await?;
    Ok(see_other("/"))
}
