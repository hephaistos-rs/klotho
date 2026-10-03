use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, attributes, class, component, view},
};

use super::label::label;

/// The layout of a [`field`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldOrientation {
    /// Stack the label, control, and supporting text.
    #[default]
    Vertical,
}

impl FieldOrientation {
    fn classes(self) -> StaticClass {
        match self {
            Self::Vertical => class!("flex-col gap-1.5"),
        }
    }
}

/// The text size of a [`field_legend`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldLegendVariant {
    /// A heading for a section of the form.
    #[default]
    Legend,
    /// A smaller heading matching a field label.
    Label,
}

impl FieldLegendVariant {
    fn classes(self) -> StaticClass {
        match self {
            Self::Legend => class!("text-base font-semibold"),
            Self::Label => class!("text-sm font-medium"),
        }
    }
}

/// A semantic group of related controls, named by a [`field_legend`]. In a
/// form's 16px stack (`flex flex-col gap-4`) it sits 24px below the field before it.
///
/// Attributes are forwarded to the `<fieldset>`. Pass `disabled` to disable
/// its controls together. Classes are appended to the component's classes,
/// as with the other field components.
#[component]
pub async fn field_set(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <fieldset
            class=(class!(
                "group/fieldset flex min-w-0 flex-col gap-4 not-first:mt-2",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </fieldset>
    })
}

/// The accessible heading of a [`field_set`]. Place it first in the set. It
/// turns madder, like a [`field_label`], when the set has `data-invalid`.
#[component]
pub async fn field_legend(
    #[default] variant: FieldLegendVariant,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <legend
            class=(class!(
                "mb-3 group-data-invalid/fieldset:text-destructive-ink",
                variant.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </legend>
    })
}

/// A control and its label, description, and optional error message.
///
/// Compose it with existing inputs, selects, checkboxes, or other controls.
/// Set `for` on [`field_label`] to the control's `id`, and connect descriptions
/// and errors through the control's `aria-describedby`. Set `aria-invalid`
/// to `"true"` on an invalid control; the field then colors its label too.
/// A `data-invalid="true"` attribute on the field also colors its label.
/// Validation and the visibility of error messages belong to the caller.
/// Attributes are forwarded to the wrapper and classes are appended.
#[component]
pub async fn field(
    #[default] orientation: FieldOrientation,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            data-slot="field"
            class=(class!(
                "group/field flex min-w-0",
                orientation.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </div>
    })
}

/// A [`label`] that follows its field's disabled and invalid states.
///
/// Pass `for` to associate the label with a control's `id`.
#[component]
pub async fn field_label(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        label(
            attrs: attributes! {
                data-slot="field-label"
                class=(class!(
                    "leading-snug group-has-[:disabled]/field:opacity-50 \
                     group-has-[[aria-invalid=true]]/field:text-destructive-ink \
                     group-data-[invalid=true]/field:text-destructive-ink",
                    attrs.remove("class"),
                ))
                (attrs)
            },
            (child)
        )
    })
}

/// Supporting text. Give it an `id` referenced by the control's `aria-describedby`.
#[component]
pub async fn field_description(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!(
                "text-meta text-muted-foreground [&_a]:text-primary [&_a]:underline \
                 [&_a]:underline-offset-4",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </p>
    })
}

/// An error message for a field, announced when it appears.
///
/// Render it only when there is an error. Give it an `id` referenced by the
/// control's `aria-describedby`, and set `aria-invalid="true"` on the control.
/// Child content can be a message or a list of messages.
#[component]
pub async fn field_error(#[default] mut attrs: Attributes, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <div
            role="alert"
            class=(class!(
                "text-meta font-medium text-destructive-ink",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </div>
    })
}
