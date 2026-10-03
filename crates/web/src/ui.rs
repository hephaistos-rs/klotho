//! The Klotho shell (header, navigation, page frame, footer) and small pieces
//! of markup the pages share. Controls come from the vendored Topcoat UI
//! components in `crate::components`; this module only composes them.

use klotho_core::User;
use topcoat::Result;
use topcoat::icon::iconify::iconify_icon;
use topcoat::icon::{IconData, icon};
use topcoat::view::{Child, View, attributes, component, view};

use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::field::{field as field_wrapper, field_label};
use crate::components::input::input;
use crate::theme::Theme;

const ALERT_ICON: IconData = iconify_icon!("lucide:circle-alert");

/// Header links on the indigo shell. The current page gets the thread: a
/// weld underline (DESIGN.md P2), plus `aria-current="page"`.
const SHELL_LINK: &str = "rounded-md px-2 py-1.5 text-sm font-medium text-shell-muted \
     transition-colors outline-none hover:text-shell-foreground \
     focus-visible:ring-2 focus-visible:ring-shell-foreground \
     aria-[current=page]:text-shell-foreground aria-[current=page]:underline \
     aria-[current=page]:decoration-thread aria-[current=page]:decoration-2 \
     aria-[current=page]:underline-offset-8";

/// The page shell: header with navigation, the page frame, and the footer
/// with the theme switch. `path` is the current request path.
#[component]
pub async fn shell(
    user: Option<User>,
    theme: Theme,
    path: String,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let tokens_current = (path == "/-/settings/tokens").then_some("page");
    let login_current = (path == "/-/login").then_some("page");
    let register_current = (path == "/-/register").then_some("page");
    Ok(view! {
        <a
            href="#main"
            class="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-50 \
                   focus:rounded-md focus:bg-background focus:px-3 focus:py-2 focus:text-sm"
        >
            "Skip to content"
        </a>
        <header class="border-b-2 border-thread bg-shell text-shell-foreground">
            <nav
                aria-label="Main"
                class="mx-auto flex h-13 max-w-6xl items-center justify-between gap-4 px-4 sm:px-6"
            >
                <a
                    href="/"
                    class="rounded-md text-lg font-semibold tracking-tight outline-none \
                           focus-visible:ring-2 focus-visible:ring-shell-foreground"
                >
                    "Klotho"
                </a>
                match user {
                    Some(user) => {
                        <div class="flex items-center gap-1">
                            <a
                                href="/-/settings/tokens"
                                class=(SHELL_LINK)
                                aria-current=(tokens_current)
                            >
                                "Access tokens"
                            </a>
                            <span class="px-2 text-sm text-shell-muted">
                                (user.username)
                            </span>
                            <form method="post" action="/-/logout">
                                <button type="submit" class=(SHELL_LINK)>
                                    "Sign out"
                                </button>
                            </form>
                        </div>
                    }
                    None => {
                        <div class="flex items-center gap-1">
                            <a
                                href="/-/login"
                                class=(SHELL_LINK)
                                aria-current=(login_current)
                            >
                                "Sign in"
                            </a>
                            <a
                                href="/-/register"
                                class=(SHELL_LINK)
                                aria-current=(register_current)
                            >
                                "Register"
                            </a>
                        </div>
                    }
                }
            </nav>
        </header>
        <main id="main" class="mx-auto max-w-6xl px-4 py-8 sm:px-6">(child)</main>
        <footer class="mx-auto max-w-6xl px-4 pb-8 sm:px-6">
            theme_switch(theme: theme, path: path)
        </footer>
    })
}

/// System, light or dark: three submit buttons in one form, so it works
/// without JavaScript (NFR-UI-005).
#[component]
async fn theme_switch(theme: Theme, path: String) -> Result<impl View> {
    Ok(view! {
        <form
            method="post"
            action="/-/theme"
            class="flex items-center gap-2 text-meta text-muted-foreground"
        >
            <input type="hidden" name="return_to" value=(path)>
            <span id="theme-label">"Theme"</span>
            <div role="group" aria-labelledby="theme-label" class="flex gap-1">
                for option in Theme::ALL {
                    let pressed = if option == theme { "true" } else { "false" };
                    button(
                        variant: if option == theme {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        },
                        size: ButtonSize::Sm,
                        attrs: attributes! {
                            type="submit"
                            name="theme"
                            value=(option.as_str())
                            aria-pressed=(pressed)
                        },
                        (option.label())
                    )
                }
            </div>
        </form>
    })
}

#[component]
pub async fn page_title(title: &str) -> Result<impl View> {
    Ok(view! { <h1 class="text-2xl font-semibold tracking-tight">(title)</h1> })
}

/// A labelled, required input. `autocomplete` helps password managers fill
/// it in.
#[component]
pub async fn field(
    name: &str,
    label: &str,
    kind: &str,
    value: &str,
    autocomplete: &str,
) -> Result<impl View> {
    let id = format!("field-{name}");
    Ok(view! {
        field_wrapper(
            field_label(attrs: attributes! { for=(id.clone()) }, (label))
            input(
                attrs: attributes! {
                    id=(id)
                    type=(kind)
                    name=(name)
                    value=(value)
                    autocomplete=(autocomplete)
                    required=""
                }
            )
        )
    })
}

/// A form-level error, announced when the page loads with it.
#[component]
pub async fn form_error(error: Option<String>) -> Result<impl View> {
    Ok(view! {
        if let Some(error) = error {
            alert(
                variant: AlertVariant::Destructive,
                attrs: attributes! { role="alert" },
                icon(data: ALERT_ICON)
                alert_title((error))
            )
        }
    })
}
