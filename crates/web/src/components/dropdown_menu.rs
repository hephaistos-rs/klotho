use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// A floating action menu controlled by a trigger.
///
/// Uses a native `<details>` element, so the trigger opens and closes it without
/// JavaScript. Closing it on an outside click requires application scripting. `attrs`
/// are forwarded to the `<details>`, with extra classes added to its classes.
/// Items use normal Tab navigation. The component does not implement the ARIA menu
/// pattern's arrow-key navigation.
///
/// ```ignore
/// view! {
///     dropdown_menu(
///         dropdown_menu_trigger("Options")
///         dropdown_menu_content(
///             dropdown_menu_item("Rename")
///             dropdown_menu_item("Duplicate")
///             dropdown_menu_separator()
///             dropdown_menu_item(
///                 attrs: attributes! { class="text-destructive-ink" },
///                 "Delete"
///             )
///         )
///     )
/// }
/// ```
#[component]
pub async fn dropdown_menu(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <details
            class=(class!("group relative inline-block", attrs.remove("class")))
            (attrs)
        >
            (child)
        </details>
    })
}

/// Classes that hide the native disclosure marker and show a pointer cursor.
/// The focus ring is the `--ring` style every control uses.
const TRIGGER: StaticClass = class!(
    "cursor-pointer list-none focus-visible:outline-hidden [&::-webkit-details-marker]:hidden \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     focus-visible:ring-offset-background",
);

/// A trigger that opens or closes the dropdown menu.
///
/// Pass its label as children. To style it as a button, pass classes from
/// [`button_variants`](super::button::button_variants) through `attrs`. The attributes
/// go on the `<summary>`. Use `group-open:` classes to style children while the menu is
/// open.
///
/// ```ignore
/// view! {
///     dropdown_menu_trigger(
///         attrs: attributes! {
///             class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
///         },
///         "Options"
///         icon(
///             data: iconify_icon!("lucide:chevron-down"),
///             attrs: attributes! { class="group-open:rotate-180" }
///         )
///     )
/// }
/// ```
#[component]
pub async fn dropdown_menu_trigger(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <summary class=(class!(TRIGGER, attrs.remove("class"))) (attrs)>
            (child)
        </summary>
    })
}

/// Classes for floating menu panels with their own background, border, and text color.
/// Menus float, so they cast the float shadow.
const PANEL: StaticClass = class!(
    "absolute z-50 min-w-48 rounded-lg border border-border bg-popover p-1 \
     text-popover-foreground shadow-sm",
);

/// Which edge of the trigger a [`dropdown_menu_content`] lines up with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropdownMenuAlign {
    /// The panel's left edge meets the trigger's left edge.
    #[default]
    Start,
    /// The panel's right edge meets the trigger's right edge, for triggers at the
    /// right of a row or header.
    End,
}

impl DropdownMenuAlign {
    fn classes(self) -> StaticClass {
        match self {
            Self::Start => class!("top-full left-0 mt-1"),
            Self::End => class!("top-full right-0 mt-1"),
        }
    }
}

/// The floating panel of a [`dropdown_menu`], holding the menu's items.
///
/// The panel drops directly below the trigger, aligned to its left edge, or to its
/// right edge with `align: DropdownMenuAlign::End`.
#[component]
pub async fn dropdown_menu_content(
    #[default] align: DropdownMenuAlign,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(PANEL, align.classes(), attrs.remove("class"))) (attrs)>
            (child)
        </div>
    })
}

/// Classes for a menu item and its interaction states. The current item (a link with
/// `aria-current="page"`) takes the gold wash and the action colour.
const ITEM: StaticClass = class!(
    "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left max-sm:min-h-10 \
     text-sm whitespace-nowrap text-popover-foreground transition-colors duration-150 \
     focus-visible:outline-hidden [&_svg]:size-4 [&_svg]:shrink-0 \
     hover:bg-accent focus-visible:bg-accent focus-visible:ring-2 \
     focus-visible:ring-ring focus-visible:ring-inset active:bg-primary/15 \
     aria-[current=page]:bg-accent aria-[current=page]:font-medium \
     aria-[current=page]:text-primary \
     disabled:pointer-events-none disabled:opacity-50",
);

/// One action in a [`dropdown_menu_content`], rendered as a `<button>`. Put it in a
/// `<form method="post">` for a state change.
#[component]
pub async fn dropdown_menu_item(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <button class=(class!(ITEM, attrs.remove("class"))) (attrs)>(child)</button>
    })
}

/// One link in a [`dropdown_menu_content`], rendered as an `<a>`. Pass `href` (and
/// `aria-current="page"` for the current page) in `attrs`.
#[component]
pub async fn dropdown_menu_link(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <a class=(class!(ITEM, attrs.remove("class"))) (attrs)>(child)</a> })
}

/// A non-interactive heading grouping the items after it.
#[component]
pub async fn dropdown_menu_label(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!(
                "px-2 py-1.5 text-meta font-medium text-muted-foreground",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </p>
    })
}

/// A hairline rule separating groups of items.
#[component]
pub async fn dropdown_menu_separator(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <hr class=(class!("-mx-1 my-1 border-border", attrs.remove("class"))) (attrs)>
    })
}
