use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// The visual style of a [`badge`].
///
/// [`Default`] is `BadgeVariant::Secondary`, used when no variant is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// A quiet grey fill for neutral statuses ("Admin", "Private").
    #[default]
    Secondary,
    /// Madder wash and ink: "Expired", "Revoked", "Failed".
    Destructive,
    /// Olive wash and ink: "Active", "Merged", "Passing".
    Success,
    /// Amber wash and ink. [`badge`] adds a warning icon, so it is never colour
    /// alone.
    Warning,
    /// A ref chip: a branch, tag or scope name in mono. Not for statuses. A long
    /// name is cut with an ellipsis rather than widening the page, so pass the full
    /// name in `title` too: `attrs: attributes! { title=(name.clone()) }`.
    Ref,
}

impl BadgeVariant {
    /// Classes for the variant, including its shape, border and colours. Keep them out
    /// of the shared base to avoid conflicting classes.
    fn classes(self) -> StaticClass {
        match self {
            Self::Secondary => class!(
                "inline-flex shrink-0 items-center justify-center gap-1 rounded-md border-transparent bg-secondary px-2 py-0.5 font-medium \
                 text-secondary-foreground",
            ),
            Self::Destructive => class!(
                "inline-flex shrink-0 items-center justify-center gap-1 rounded-md border-transparent bg-destructive-soft px-2 py-0.5 font-medium \
                 text-destructive-ink",
            ),
            Self::Success => class!(
                "inline-flex shrink-0 items-center justify-center gap-1 rounded-md border-transparent bg-success-soft px-2 py-0.5 font-medium \
                 text-success-ink",
            ),
            Self::Warning => class!(
                "inline-flex shrink-0 items-center justify-center gap-1 rounded-md border-transparent bg-warning-soft px-2 py-0.5 font-medium \
                 text-warning-ink",
            ),
            Self::Ref => class!(
                "inline-block min-w-0 overflow-hidden text-ellipsis align-bottom rounded-sm border-border bg-muted px-1.5 py-px font-mono font-medium \
                 text-foreground",
            ),
        }
    }
}

/// Classes shared by badge variants. A border reserves the same space in every variant.
const BASE: StaticClass = class!(
    "w-fit max-w-full border \
     text-meta whitespace-nowrap [&>svg]:size-3.5 [&>svg]:shrink-0",
);

/// A small label for a status, a count or a ref.
///
/// `variant` defaults to `Secondary`. Pass the label as children (sentence case) and
/// extra attributes through `attrs`. Attributes go on the `<span>`, with classes added
/// to its classes.
///
/// ```ignore
/// view! {
///     badge(variant: BadgeVariant::Destructive, "Expired")
///     badge(variant: BadgeVariant::Ref, "refs/heads/main")
/// }
/// ```
#[component]
pub async fn badge(
    #[default] variant: BadgeVariant,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <span class=(class!(BASE, variant.classes(), attrs.remove("class"))) (attrs)>
            if variant == BadgeVariant::Warning {
                icon(data: iconify_icon!("lucide:triangle-alert"))
            }
            (child)
        </span>
    })
}
