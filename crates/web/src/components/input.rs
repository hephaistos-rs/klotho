use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

/// Classes for the input's dimensions, border, and interaction states. The
/// `--input` border meets 3:1 against the card in both themes.
const INPUT: StaticClass = class!(
    "h-9 w-full min-w-0 rounded-md border border-input bg-card px-3 \
     text-base text-foreground transition-colors duration-150 focus-visible:outline-hidden sm:text-sm \
     placeholder:text-muted-foreground hover:border-muted-foreground \
     file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-sm file:font-medium \
     focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring \
     focus-visible:ring-offset-2 focus-visible:ring-offset-background \
     aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive \
     [&[readonly]]:bg-muted [&[readonly]]:hover:border-input \
     disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted \
     disabled:opacity-50",
);

/// A styled input.
///
/// Pass input attributes and event handlers through `attrs`. Extra classes are added to
/// the input's classes. It fills its container by default. Set `aria-invalid="true"` to
/// show the error border and focus ring. A `readonly` input (a secret or a clone URL to
/// copy) gets the muted fill; add `font-mono` for those.
///
/// ```ignore
/// view! {
///     input(attrs: attributes! { type="email" placeholder="you@example.com" })
/// }
/// ```
#[component]
pub async fn input(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! { <input class=(class!(INPUT, attrs.remove("class"))) (attrs)> })
}
