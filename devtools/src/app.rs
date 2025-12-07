//! Main application component with routing.

use leptos::*;
use leptos_router::*;

use crate::components::{Metrics, Playground, Sidebar, Tasks, Workers};

/// Main application component.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <div class="flex h-screen">
                <Sidebar />
                <main class="flex-1 overflow-auto p-6">
                    <Routes>
                        <Route path="/" view=Workers />
                        <Route path="/workers" view=Workers />
                        <Route path="/tasks" view=Tasks />
                        <Route path="/playground" view=Playground />
                        <Route path="/metrics" view=Metrics />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}
