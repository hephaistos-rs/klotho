//! Small pieces of markup the pages share.

use topcoat::Result;
use topcoat::view::{View, component, view};

#[component]
pub async fn page_title(title: &str) -> Result<impl View> {
    Ok(view! { <h1 class="text-2xl font-bold">(title)</h1> })
}

/// A labelled input. `autocomplete` helps password managers fill it in.
#[component]
pub async fn field(
    name: &str,
    label: &str,
    kind: &str,
    value: &str,
    autocomplete: &str,
) -> Result<impl View> {
    Ok(view! {
        <label class="block">
            <span class="text-sm font-medium">(label)</span>
            <input
                type=(kind)
                name=(name)
                value=(value)
                autocomplete=(autocomplete)
                required=""
                class="mt-1 block w-full rounded border border-gray-300 px-3 py-2"
            >
        </label>
    })
}

#[component]
pub async fn form_error(error: Option<String>) -> Result<impl View> {
    Ok(view! {
        if let Some(error) = error {
            <p role="alert" class="rounded bg-red-50 px-3 py-2 text-sm text-red-800">(error)</p>
        }
    })
}
