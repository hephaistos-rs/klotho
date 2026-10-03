use topcoat::{
    Result,
    icon::{IconData, icon, iconify::iconify_icon},
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// The visual style of an [`alert`].
///
/// [`Default`] is `AlertVariant::Neutral`, used when no variant is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// A plain notice on the card colour. Pass your own icon if it needs one.
    #[default]
    Neutral,
    /// Madder wash and ink, for errors and failures.
    Destructive,
    /// Olive wash and ink, for something that worked (a token created).
    Success,
}

impl AlertVariant {
    /// Classes for the variant's fill, border and text colours. Soft fills with a
    /// full hairline border, never a coloured side stripe.
    fn classes(self) -> StaticClass {
        match self {
            Self::Neutral => class!(
                "border-border bg-card text-card-foreground [&>svg]:text-muted-foreground \
                 [&_[data-slot=alert-description]]:text-muted-foreground",
            ),
            Self::Destructive => {
                class!("border-destructive/30 bg-destructive-soft text-destructive-ink")
            }
            Self::Success => class!("border-success/30 bg-success-soft text-success-ink"),
        }
    }

    /// The icon every semantic variant carries, so state is never shown by colour
    /// alone. `Neutral` has none.
    fn icon(self) -> Option<IconData> {
        match self {
            Self::Neutral => None,
            Self::Destructive => Some(iconify_icon!("lucide:circle-alert")),
            Self::Success => Some(iconify_icon!("lucide:circle-check")),
        }
    }
}

/// Classes for the alert layout. The icon column collapses when no icon is present.
const BASE: StaticClass = class!(
    "grid w-full grid-cols-[0_1fr] items-start gap-y-1 rounded-lg border \
     px-4 py-3 text-sm has-[>svg]:grid-cols-[1rem_1fr] has-[>svg]:gap-x-3 \
     [&>svg]:size-4 [&>svg]:translate-y-0.5",
);

/// A notice displayed within the page.
///
/// Use `variant` to choose its style. `Destructive` and `Success` render their
/// own icon, so pass only an `alert_title` and an optional `alert_description`.
/// A `Neutral` alert takes an optional icon first. `attrs` are forwarded to the
/// `<div>`, with extra classes added to its classes. A warning variant (amber
/// wash and ink with a `triangle-alert` icon, DESIGN.md) gets added when a page
/// first needs one.
///
/// ```ignore
/// view! {
///     alert(
///         variant: AlertVariant::Success,
///         alert_title("Token created")
///         alert_description("Copy it now. It won't be shown again.")
///     )
/// }
/// ```
#[component]
pub async fn alert(
    /// The visual style of the notice.
    #[default]
    variant: AlertVariant,
    /// Extra attributes for the `<div>` element.
    #[default]
    mut attrs: Attributes,
    /// The alert's title and description (and icon, for `Neutral`).
    #[default]
    child: Child<'_>,
) -> Result<impl View> {
    // `role="alert"` is deliberately absent: it interrupts a screen reader
    // the moment the element appears, which suits a message arriving during
    // the visit, not one rendered with the page. Pass it among the `attrs`
    // where that is what you want.
    Ok(view! {
        <div class=(class!(BASE, variant.classes(), attrs.remove("class"))) (attrs)>
            if let Some(data) = variant.icon() {
                icon(data: data)
            }
            (child)
        </div>
    })
}

/// The heading of an alert.
#[component]
pub async fn alert_title(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <p
            data-slot="alert-title"
            class=(class!("col-start-2 font-medium", attrs.remove("class")))
            (attrs)
        >
            (child)
        </p>
    })
}

/// Text that explains the alert and any action the reader should take. It takes the
/// variant's ink colour (muted on a `Neutral` alert).
#[component]
pub async fn alert_description(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            data-slot="alert-description"
            class=(class!("col-start-2 min-w-0 text-sm", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    })
}
