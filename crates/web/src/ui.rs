//! The Klotho shell (header, navigation, page frame, footer) and small pieces
//! of markup the pages share. Controls come from the vendored Topcoat UI
//! components in `crate::components`; this module only composes them.

use klotho_core::User;
use topcoat::Result;
use topcoat::icon::iconify::iconify_icon;
use topcoat::icon::{IconData, icon};
use topcoat::view::{Attributes, Child, View, attributes, class, component, view};

use crate::components::alert::{AlertVariant, alert, alert_description, alert_title};
use crate::components::button::{ButtonSize, ButtonVariant, button, button_variants};
use crate::components::dropdown_menu::{
    DropdownMenuAlign, dropdown_menu, dropdown_menu_content, dropdown_menu_item, dropdown_menu_label,
    dropdown_menu_link, dropdown_menu_separator, dropdown_menu_trigger,
};
use crate::components::field::{field as field_wrapper, field_description, field_error, field_label};
use crate::components::input::input;
use crate::theme::Theme;

const MENU_ICON: IconData = iconify_icon!("lucide:menu");

/// A link in the main navigation.
struct NavLink {
    href: &'static str,
    label: &'static str,
}

/// Navigation for signed-in readers. On wide screens the links sit in the
/// header; below 640px they fold into a `<details>` menu (DESIGN.md P5).
/// Add a link here and both places get it.
const MEMBER_LINKS: &[NavLink] = &[NavLink { href: "/-/settings/tokens", label: "Access tokens" }];

/// The current-nav marker: the thread, a 2px gold underline drawn by CSS
/// (DESIGN.md P2). The link itself carries `aria-current="page"`.
const THREAD_MARKER: &str = "aria-[current=page]:underline aria-[current=page]:decoration-thread \
     aria-[current=page]:decoration-2 aria-[current=page]:underline-offset-8";

/// `aria-current="page"` when `href` is the page being shown.
fn current(path: &str, href: &str) -> Option<&'static str> {
    (path == href).then_some("page")
}

/// The page shell: header with navigation, the page frame, and the footer
/// with the theme switch. `path` is the current request path.
#[component]
pub async fn shell(
    user: Option<User>,
    theme: Theme,
    path: String,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let login_current = current(&path, "/-/login");
    let register_current = current(&path, "/-/register");
    let links: Vec<(&'static str, &'static str, Option<&'static str>)> =
        MEMBER_LINKS.iter().map(|link| (link.href, link.label, current(&path, link.href))).collect();
    let menu_links = links.clone();
    Ok(view! {
        <div class="flex min-h-dvh flex-col">
            <a
                href="#main"
                class="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-50 \
                       focus:rounded-md focus:border focus:border-border focus:bg-popover \
                       focus:px-3 focus:py-2 focus:text-sm focus:font-medium \
                       focus:text-popover-foreground focus:shadow-sm focus-visible:outline-hidden \
                       focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
                       focus-visible:ring-offset-background"
            >
                "Skip to content"
            </a>
            <header class="border-b-2 border-thread bg-shell text-shell-foreground">
                <nav
                    aria-label="Main"
                    class="mx-auto flex h-13 max-w-6xl items-center justify-between gap-3 px-4 sm:px-6"
                >
                    <a
                        href="/"
                        class="inline-flex shrink-0 items-center gap-2 rounded-md font-display text-2xl \
                               leading-none font-semibold focus-visible:outline-hidden focus-visible:ring-2 \
                               focus-visible:ring-ring focus-visible:ring-offset-2 \
                               focus-visible:ring-offset-background"
                    >
                        spindle()
                        "Klotho"
                    </a>
                    match user {
                        Some(user) => {
                            <div class="flex min-w-0 items-center gap-1">
                                <div class="hidden items-center gap-1 sm:flex">
                                    for (href, label, here) in links {
                                        <a
                                            href=(href)
                                            class=(class!(
                                                button_variants(ButtonVariant::Shell, ButtonSize::Sm),
                                                THREAD_MARKER,
                                            ))
                                            aria-current=(here)
                                        >
                                            (label)
                                        </a>
                                    }
                                </div>
                                // Who is signed in: set apart from the links by a
                                // rule and in meta size, so it reads as identity,
                                // not as a link.
                                <span
                                    class="ml-1 hidden max-w-40 truncate border-l border-shell-foreground/20 \
                                           pr-1 pl-3 text-meta text-shell-muted sm:inline"
                                >
                                    <span class="sr-only">"Signed in as "</span>
                                    (user.username.clone())
                                </span>
                                // Below 640px Sign out lives in the menu, away from
                                // the Menu button.
                                <form
                                    method="post"
                                    action="/-/logout"
                                    class="hidden sm:block"
                                >
                                    button(
                                        variant: ButtonVariant::Shell,
                                        size: ButtonSize::Sm,
                                        attrs: attributes! { type="submit" },
                                        "Sign out"
                                    )
                                </form>
                                dropdown_menu(
                                    attrs: attributes! { class="sm:hidden" },
                                    dropdown_menu_trigger(
                                        attrs: attributes! {
                                            class=(class!(
                                                button_variants(ButtonVariant::Shell, ButtonSize::Sm),
                                                "max-sm:h-10",
                                            ))
                                        },
                                        icon(data: MENU_ICON)
                                        "Menu"
                                    )
                                    dropdown_menu_content(
                                        align: DropdownMenuAlign::End,
                                        dropdown_menu_label(
                                            <span class="block max-w-56 truncate">
                                                "Signed in as "
                                                (user.username)
                                            </span>
                                        )
                                        for (href, label, here) in menu_links {
                                            dropdown_menu_link(
                                                attrs: attributes! { href=(href) aria-current=(here) },
                                                (label)
                                            )
                                        }
                                        dropdown_menu_separator()
                                        <form method="post" action="/-/logout">
                                            dropdown_menu_item(
                                                attrs: attributes! { type="submit" },
                                                "Sign out"
                                            )
                                        </form>
                                    )
                                )
                            </div>
                        }
                        None => {
                            <div class="flex items-center gap-1">
                                <a
                                    href="/-/login"
                                    class=(class!(
                                        button_variants(ButtonVariant::Shell, ButtonSize::Sm),
                                        "max-sm:h-10",
                                        THREAD_MARKER,
                                    ))
                                    aria-current=(login_current)
                                >
                                    "Sign in"
                                </a>
                                <a
                                    href="/-/register"
                                    class=(class!(
                                        button_variants(ButtonVariant::Shell, ButtonSize::Sm),
                                        "max-sm:h-10",
                                        THREAD_MARKER,
                                    ))
                                    aria-current=(register_current)
                                >
                                    "Register"
                                </a>
                            </div>
                        }
                    }
                </nav>
            </header>
            <main id="main" class="mx-auto w-full max-w-6xl flex-1 px-4 py-8 sm:px-6">
                (child)
            </main>
            <footer class="border-t border-border">
                <div class="mx-auto max-w-6xl px-4 py-4 sm:px-6">
                    theme_switch(theme: theme, path: path)
                </div>
            </footer>
        </div>
    })
}

/// The mark beside the wordmark: a drop spindle, its whorl, and the gold
/// thread coming off the top. Decorative, so hidden from assistive tech.
#[component]
async fn spindle() -> Result<impl View> {
    Ok(view! {
        <svg
            class="size-6 shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke-width="1.5"
            stroke-linecap="round"
            aria-hidden="true"
        >
            <path d="M12 2v20" stroke="currentColor"></path>
            <ellipse cx="12" cy="15" rx="7" ry="3" stroke="currentColor"></ellipse>
            <path d="M12 3c4 0 6 2.5 10 1.5" class="stroke-thread"></path>
        </svg>
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
            class="flex flex-wrap items-center gap-2 text-meta text-muted-foreground"
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
                            class="max-sm:h-10 aria-pressed:bg-accent \
                                   aria-[pressed=false]:text-muted-foreground \
                                   aria-[pressed=false]:hover:text-foreground"
                        },
                        icon(data: theme_icon(option))
                        (option.label())
                    )
                }
            </div>
        </form>
    })
}

fn theme_icon(theme: Theme) -> IconData {
    match theme {
        Theme::System => iconify_icon!("lucide:monitor"),
        Theme::Light => iconify_icon!("lucide:sun"),
        Theme::Dark => iconify_icon!("lucide:moon"),
    }
}

/// The page's `<h1>`. Use [`page_header`] when the page has a description or
/// actions.
#[component]
pub async fn page_title(title: &str) -> Result<impl View> {
    Ok(view! {
        <h1 class="font-display text-3xl leading-tight font-semibold sm:text-4xl">
            (title)
        </h1>
    })
}

/// The top of a page: the `<h1>`, an optional one-line description, and the
/// page's actions (the children) on the right, wrapping below under 640px.
#[component]
pub async fn page_header(
    title: &str,
    #[into]
    #[default]
    description: Option<&str>,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class="mb-6 flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6"
        >
            <div class="min-w-0">
                page_title(title: title)
                if let Some(description) = description {
                    <p class="mt-1 max-w-prose text-muted-foreground">(description)</p>
                }
            </div>
            <div class="flex shrink-0 flex-wrap items-center gap-2 empty:hidden">
                (child)
            </div>
        </div>
    })
}

/// A section's `<h2>`: 40px above, 12px below, with optional actions (the
/// children) on the right. `id` lets a list name itself with `aria-labelledby`.
#[component]
pub async fn section_heading(
    title: &str,
    #[into]
    #[default]
    id: Option<&str>,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class="mt-10 mb-3 flex flex-wrap items-center justify-between gap-2">
            <h2 id=(id) class="text-lg font-semibold">(title)</h2>
            <div class="flex flex-wrap items-center gap-2 empty:hidden">(child)</div>
        </div>
    })
}

/// A bordered list for a collection (tokens, repositories, branches): one
/// `<ul>` with hairline dividers. `attrs` go on the `<ul>` (pass
/// `aria-labelledby` to name it after its section heading).
#[component]
pub async fn list(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <ul
            class=(class!(
                "divide-y divide-border rounded-lg border border-border bg-card",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </ul>
    })
}

/// One row of a [`list`]. Pass the content first and the actions last: they sit
/// side by side, and under 640px the actions wrap below the content at their
/// natural width, so the row reads content first.
#[component]
pub async fn list_row(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <li
            class=(class!(
                "flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between \
                 sm:gap-4 [&>*:first-child]:min-w-0 max-sm:[&>*:not(:first-child)]:self-start",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </li>
    })
}

/// What an empty collection shows instead of a list: what's missing (`title`)
/// and, as children, what to do next.
#[component]
pub async fn empty_state(title: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <div class="rounded-lg border border-border bg-card px-4 py-6">
            <p class="font-medium">(title)</p>
            <div class="mt-1 text-sm text-muted-foreground empty:hidden">(child)</div>
        </div>
    })
}

/// A soft notice with its icon (success, error, or neutral), a title,
/// and optional details as children. Announced politely as a status.
#[component]
pub async fn notice(
    #[default] variant: AlertVariant,
    title: &str,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        alert(
            variant: variant,
            attrs: attributes! { role="status" },
            alert_title((title))
            alert_description(attrs: attributes! { class="empty:hidden" }, (child))
        )
    })
}

/// A labelled, required input. `autocomplete` helps password managers fill
/// it in. An optional `hint` shows below the input; an `error` marks it
/// invalid and is tied to it with `aria-describedby`.
#[component]
pub async fn field(
    name: &str,
    label: &str,
    kind: &str,
    value: &str,
    autocomplete: &str,
    #[into]
    #[default]
    hint: Option<&str>,
    #[into]
    #[default]
    error: Option<&str>,
) -> Result<impl View> {
    let id = format!("field-{name}");
    let hint_id = hint.map(|_| format!("{id}-hint"));
    let error_id = error.map(|_| format!("{id}-error"));
    let described_by = match (&hint_id, &error_id) {
        (Some(hint), Some(error)) => Some(format!("{hint} {error}")),
        (hint, error) => hint.clone().or_else(|| error.clone()),
    };
    let invalid = error.is_some().then_some("true");
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
                    aria-invalid=(invalid)
                    aria-describedby=(described_by)
                }
            )
            if let Some(hint) = hint {
                field_description(attrs: attributes! { id=(hint_id) }, (hint))
            }
            if let Some(error) = error {
                field_error(attrs: attributes! { id=(error_id) }, (error))
            }
        )
    })
}

/// A form's submit row: the primary button first, then any secondary action.
/// In a 16px form stack (`flex flex-col gap-4`) it sits 24px below the last field.
#[component]
pub async fn form_actions(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <div class="mt-2 flex flex-wrap items-center gap-3">(child)</div> })
}

/// A form-level error, announced when the page loads with it.
#[component]
pub async fn form_error(error: Option<String>) -> Result<impl View> {
    Ok(view! {
        if let Some(error) = error {
            alert(
                variant: AlertVariant::Destructive,
                attrs: attributes! { role="alert" },
                alert_title((error))
            )
        }
    })
}

/// A value to read or copy character by character (a secret, a clone URL, a
/// command) in a mono block. It scrolls sideways on its own, or with `wrap`
/// breaks anywhere so a secret is fully visible. The scrolling form is
/// focusable, so keyboard users can scroll it. It sits on the card colour with
/// a hairline border, so it reads on any surface, alerts included.
#[component]
pub async fn code_line(value: &str, #[default] wrap: bool) -> Result<impl View> {
    let flow = if wrap { "break-all whitespace-pre-wrap" } else { "overflow-x-auto whitespace-pre" };
    let tabindex = (!wrap).then_some("0");
    Ok(view! {
        <code
            tabindex=(tabindex)
            class=(class!(
                "block max-w-full rounded-md border border-border bg-card px-3 py-2 font-mono \
                 text-meta text-foreground",
                flow,
            ))
        >
            (value)
        </code>
    })
}
