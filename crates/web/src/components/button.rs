use topcoat::{
    Result,
    view::{Attributes, Child, Class, StaticClass, View, class, component, view},
};

/// The visual style of a [`button`].
///
/// [`Default`] is `ButtonVariant::Primary`, used when no variant is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The action-colour button (bronze by day, gold at night) for the main action. One per form.
    #[default]
    Primary,
    /// A card-coloured button with a control border, for secondary actions.
    Secondary,
    /// A control-bordered button on whatever is behind it.
    Outline,
    /// No fill until hovered, for toolbars, menus and inline actions.
    Ghost,
    /// Solid madder. Only for the button that confirms a destructive action
    /// (the inline revoke confirmation); list rows use `DestructiveOutline`.
    Destructive,
    /// Outlined madder for a destructive action in a list ("Revoke", "Delete").
    /// Use it with [`ButtonSize::Sm`].
    DestructiveOutline,
    /// The ghost button on the night shell (header links and "Sign out").
    Shell,
}

impl ButtonVariant {
    /// Classes for the button variant and its interaction states.
    ///
    /// Each variant sets its own border and background. Keep them out of the shared
    /// base to avoid conflicting classes.
    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!(
                "border-transparent bg-primary text-primary-foreground \
                 hover:bg-primary/90 active:bg-primary/80",
            ),
            Self::Secondary => class!(
                "border-input bg-card text-foreground \
                 hover:bg-accent active:bg-primary/15",
            ),
            Self::Outline => class!(
                "border-input bg-transparent text-foreground \
                 hover:bg-accent active:bg-primary/15",
            ),
            Self::Ghost => class!(
                "border-transparent bg-transparent text-foreground \
                 hover:bg-accent active:bg-primary/15",
            ),
            Self::Destructive => class!(
                "border-transparent bg-destructive text-destructive-foreground \
                 hover:bg-destructive/90 active:bg-destructive/80",
            ),
            Self::DestructiveOutline => class!(
                "border-destructive/40 bg-card text-destructive-ink \
                 hover:border-destructive/60 hover:bg-destructive-soft \
                 active:border-destructive active:bg-destructive-soft",
            ),
            Self::Shell => class!(
                "border-transparent bg-transparent text-shell-muted \
                 hover:bg-shell-foreground/10 hover:text-shell-foreground \
                 active:bg-shell-foreground/15 aria-[current=page]:text-shell-foreground",
            ),
        }
    }
}

/// The size of a [`button`]. Every size is at least 32px tall.
///
/// [`Default`] is `ButtonSize::Md`, used when no size is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// 32px: list actions and compact toolbars.
    Sm,
    /// 36px: the standard button.
    #[default]
    Md,
}

impl ButtonSize {
    /// Classes for the button dimensions. Text size stays the same across sizes.
    fn classes(self) -> StaticClass {
        match self {
            Self::Sm => class!("h-8 gap-1.5 px-3"),
            Self::Md => class!("h-9 gap-2 px-4"),
        }
    }
}

/// Classes shared by button variants and sizes. A border reserves the same space in
/// every variant. Colour and opacity transitions only (DESIGN.md).
const BASE: StaticClass = class!(
    "inline-flex shrink-0 cursor-pointer items-center justify-center rounded-md border \
     text-sm font-medium whitespace-nowrap transition-colors duration-150 focus-visible:outline-hidden \
     select-none [&_svg]:size-4 [&_svg]:shrink-0 \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background \
     disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 \
     aria-disabled:pointer-events-none aria-disabled:opacity-50",
);

/// Builds the full class list for a button of the given `variant` and `size`.
///
/// Use it to give button styling to an element that is not a `<button>`, such
/// as a link styled as a button:
///
/// ```ignore
/// view! {
///     <a href="/login" class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>
///         "Sign in"
///     </a>
/// }
/// ```
#[must_use]
pub fn button_variants(
    variant: ButtonVariant,
    size: ButtonSize,
) -> Class<(StaticClass, StaticClass, StaticClass)> {
    class!(BASE, variant.classes(), size.classes())
}

/// A styled button.
///
/// `variant` defaults to `Primary` and `size` to `Md`. Pass the content as children.
/// `attrs` are forwarded to the `<button>`, with extra classes added to its classes.
/// Use [`button_variants`] to apply the same styling to another element.
///
/// ```ignore
/// view! {
///     button(
///         variant: ButtonVariant::DestructiveOutline,
///         size: ButtonSize::Sm,
///         attrs: attributes! { type="submit" },
///         "Revoke"
///     )
/// }
/// ```
#[component]
pub async fn button(
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <button
            class=(class!(
                BASE,
                variant.classes(),
                size.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </button>
    })
}
