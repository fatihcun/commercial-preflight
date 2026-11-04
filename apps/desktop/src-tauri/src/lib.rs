mod domain;
mod product;

use std::path::PathBuf;
use std::sync::Mutex;

use commercial_preflight_core::QuoteLifecycle;
use commercial_preflight_intake::{DocumentIntakeSource, WatchedFolderSource};
use commercial_preflight_storage::{
    CompanyDraft, ConfigurationDefaults, FolderIntakeSettings, HistoryFilter, LocalStore,
    ManagementAggregates, OnboardingRequest, OrderDetail, OrderSummary, PolicyDraft, PolicyProfile,
    ProposalTemplateProfile, QuoteRecord, ScanHistoryEntry, SetupState, TemplateDraft,
};
use domain::{
    AppStatus, AutostartStatus, ScanRequest, ScanResponse, WindowCloseAction, app_status,
    autostart_set_status, autostart_status, close_action, unexpected_scan_failure,
};
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

const BACKGROUND_ARGUMENT: &str = "--background";

struct TrayMenuState {
    autostart_item: CheckMenuItem<tauri::Wry>,
}

struct ProductState {
    store: LocalStore,
    folder_watcher: Mutex<FolderWatcherState>,
}

struct FolderWatcherState {
    settings: FolderIntakeSettings,
    source: WatchedFolderSource,
}

impl FolderWatcherState {
    fn load(store: &LocalStore) -> Self {
        let settings = store.folder_intake_settings().unwrap_or_default();
        let known_hashes = store.known_intake_hashes().unwrap_or_default();
        let source = watched_source(&settings).with_known_hashes(known_hashes);
        Self { settings, source }
    }

    fn refresh_if_changed(&mut self, store: &LocalStore) {
        let current = store.folder_intake_settings().unwrap_or_default();
        if current != self.settings {
            *self = Self::load(store);
        }
    }
}

fn watched_source(settings: &FolderIntakeSettings) -> WatchedFolderSource {
    WatchedFolderSource::new(
        settings.quote_folder.as_ref().map(PathBuf::from),
        settings.order_folder.as_ref().map(PathBuf::from),
    )
}

#[tauri::command]
async fn scan_proposal(
    state: State<'_, ProductState>,
    request: ScanRequest,
) -> Result<ScanResponse, String> {
    let path = request.path.clone();
    let store = state.store.clone();
    match tauri::async_runtime::spawn_blocking(move || product::scan_proposal(&store, request))
        .await
    {
        Ok(response) => Ok(response),
        Err(_) => Ok(unexpected_scan_failure(&path)),
    }
}

#[tauri::command]
async fn import_quote(
    state: State<'_, ProductState>,
    path: String,
    lifecycle: String,
) -> Result<product::ImportResult, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        product::import_quote(&store, &path, parse_quote_lifecycle(&lifecycle)?)
    })
    .await
    .map_err(|_| "Quote import could not be completed.".to_owned())?
}

#[tauri::command]
async fn import_customer_order(
    state: State<'_, ProductState>,
    path: String,
) -> Result<product::ImportResult, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || product::import_order(&store, &path))
        .await
        .map_err(|_| "Order import could not be completed.".to_owned())?
}

#[tauri::command]
fn get_order_inbox(state: State<'_, ProductState>) -> Result<Vec<OrderSummary>, String> {
    product::orders(&state.store)
}

#[tauri::command]
fn get_exceptions(state: State<'_, ProductState>) -> Result<Vec<OrderSummary>, String> {
    product::exceptions(&state.store)
}

#[tauri::command]
fn get_order_detail(
    state: State<'_, ProductState>,
    order_id: String,
) -> Result<OrderDetail, String> {
    state
        .store
        .order_detail(&order_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn mark_order_reviewed(
    state: State<'_, ProductState>,
    order_id: String,
) -> Result<OrderDetail, String> {
    state
        .store
        .mark_order_reviewed(&order_id)
        .map_err(product::safe_error)?;
    state
        .store
        .order_detail(&order_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn get_quotes(state: State<'_, ProductState>) -> Result<Vec<QuoteRecord>, String> {
    product::quotes(&state.store)
}

#[tauri::command]
fn set_quote_lifecycle(
    state: State<'_, ProductState>,
    quote_id: String,
    lifecycle: String,
) -> Result<Vec<QuoteRecord>, String> {
    product::set_quote_lifecycle(&state.store, &quote_id, parse_quote_lifecycle(&lifecycle)?)
}

#[tauri::command]
fn associate_quote(
    state: State<'_, ProductState>,
    order_id: String,
    quote_id: String,
) -> Result<OrderDetail, String> {
    product::manually_associate(&state.store, &order_id, &quote_id)
}

#[tauri::command]
fn get_folder_intake_settings(
    state: State<'_, ProductState>,
) -> Result<FolderIntakeSettings, String> {
    state
        .store
        .folder_intake_settings()
        .map_err(product::safe_error)
}

#[tauri::command]
fn set_folder_intake_settings(
    state: State<'_, ProductState>,
    settings: FolderIntakeSettings,
) -> Result<FolderIntakeSettings, String> {
    let saved = state
        .store
        .set_folder_intake_settings(&settings)
        .map_err(product::safe_error)?;
    if let Ok(mut watcher) = state.folder_watcher.lock() {
        *watcher = FolderWatcherState::load(&state.store);
    }
    Ok(saved)
}

#[tauri::command]
fn poll_folder_intake(
    state: State<'_, ProductState>,
) -> Result<Vec<product::ImportResult>, String> {
    let mut watcher = state
        .folder_watcher
        .lock()
        .map_err(|_| "Folder intake is temporarily unavailable.".to_owned())?;
    watcher.refresh_if_changed(&state.store);
    let candidates = watcher
        .source
        .poll()
        .map_err(|_| "Configured intake folders could not be read.".to_owned())?;
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let result = product::process_intake_candidate(
            &state.store,
            &candidate,
            (candidate.document_kind == commercial_preflight_intake::IntakeDocumentKind::Quote)
                .then_some(QuoteLifecycle::Sent),
        );
        if result.is_ok() {
            watcher.source.acknowledge(&candidate.sha256);
        }
        results.push(result?);
    }
    Ok(results)
}

#[tauri::command]
fn open_source(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_file() {
        return Err("Source document unavailable.".to_owned());
    }
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Source document could not be opened.".to_owned())
}

fn parse_quote_lifecycle(value: &str) -> Result<QuoteLifecycle, String> {
    match value {
        "DRAFT" => Ok(QuoteLifecycle::Draft),
        "SENT" => Ok(QuoteLifecycle::Sent),
        "ACCEPTED_BASELINE" => Ok(QuoteLifecycle::AcceptedBaseline),
        "SUPERSEDED" => Ok(QuoteLifecycle::Superseded),
        _ => Err("Invalid quote lifecycle.".to_owned()),
    }
}

#[tauri::command]
fn get_bootstrap(state: State<'_, ProductState>) -> Result<SetupState, String> {
    product::setup_state(&state.store)
}

#[tauri::command]
fn get_configuration_defaults() -> ConfigurationDefaults {
    ConfigurationDefaults::default()
}

#[tauri::command]
fn complete_onboarding(
    state: State<'_, ProductState>,
    request: OnboardingRequest,
) -> Result<SetupState, String> {
    state
        .store
        .complete_onboarding(&request)
        .map_err(product::safe_error)
}

#[tauri::command]
fn update_company(
    state: State<'_, ProductState>,
    company_id: String,
    draft: CompanyDraft,
) -> Result<SetupState, String> {
    state
        .store
        .update_company(&company_id, &draft)
        .map_err(product::safe_error)?;
    product::setup_state(&state.store)
}

#[tauri::command]
fn list_policies(
    state: State<'_, ProductState>,
    company_id: String,
) -> Result<Vec<PolicyProfile>, String> {
    state
        .store
        .list_policies(&company_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn create_policy(
    state: State<'_, ProductState>,
    company_id: String,
    draft: PolicyDraft,
) -> Result<PolicyProfile, String> {
    state
        .store
        .create_policy(&company_id, &draft)
        .map_err(product::safe_error)
}

#[tauri::command]
fn update_policy(
    state: State<'_, ProductState>,
    policy_id: String,
    draft: PolicyDraft,
) -> Result<PolicyProfile, String> {
    state
        .store
        .update_policy(&policy_id, &draft)
        .map_err(product::safe_error)
}

#[tauri::command]
fn set_active_policy(
    state: State<'_, ProductState>,
    company_id: String,
    policy_id: String,
) -> Result<SetupState, String> {
    state
        .store
        .set_active_policy(&company_id, &policy_id)
        .map_err(product::safe_error)?;
    product::setup_state(&state.store)
}

#[tauri::command]
fn archive_policy(state: State<'_, ProductState>, policy_id: String) -> Result<(), String> {
    state
        .store
        .archive_policy(&policy_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn list_templates(
    state: State<'_, ProductState>,
    company_id: String,
) -> Result<Vec<ProposalTemplateProfile>, String> {
    state
        .store
        .list_templates(&company_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn create_template(
    state: State<'_, ProductState>,
    company_id: String,
    draft: TemplateDraft,
) -> Result<ProposalTemplateProfile, String> {
    state
        .store
        .create_template(&company_id, &draft)
        .map_err(product::safe_error)
}

#[tauri::command]
fn update_template(
    state: State<'_, ProductState>,
    template_id: String,
    draft: TemplateDraft,
) -> Result<ProposalTemplateProfile, String> {
    state
        .store
        .update_template(&template_id, &draft)
        .map_err(product::safe_error)
}

#[tauri::command]
fn set_default_template(
    state: State<'_, ProductState>,
    company_id: String,
    template_id: String,
) -> Result<SetupState, String> {
    state
        .store
        .set_default_template(&company_id, &template_id)
        .map_err(product::safe_error)?;
    product::setup_state(&state.store)
}

#[tauri::command]
fn disable_template(state: State<'_, ProductState>, template_id: String) -> Result<(), String> {
    state
        .store
        .disable_template(&template_id)
        .map_err(product::safe_error)
}

#[tauri::command]
fn get_history(
    state: State<'_, ProductState>,
    filter: HistoryFilter,
) -> Result<Vec<ScanHistoryEntry>, String> {
    state.store.history(&filter).map_err(product::safe_error)
}

#[tauri::command]
fn get_recent_history(state: State<'_, ProductState>) -> Result<Vec<ScanHistoryEntry>, String> {
    product::recent_history(&state.store)
}

#[tauri::command]
fn clear_history(state: State<'_, ProductState>) -> Result<usize, String> {
    state.store.clear_history().map_err(product::safe_error)
}

#[tauri::command]
fn get_management_aggregates(
    state: State<'_, ProductState>,
) -> Result<ManagementAggregates, String> {
    state.store.aggregates().map_err(product::safe_error)
}

#[tauri::command]
fn get_app_status() -> AppStatus {
    app_status()
}

#[tauri::command]
fn get_autostart_status(app: AppHandle) -> AutostartStatus {
    query_autostart(&app)
}

#[tauri::command]
fn set_autostart(
    app: AppHandle,
    tray_state: State<'_, TrayMenuState>,
    enabled: bool,
) -> AutostartStatus {
    let status = apply_autostart(&app, enabled);
    let _ = tray_state.autostart_item.set_checked(status.enabled);
    status
}

fn query_autostart<R: Runtime>(app: &AppHandle<R>) -> AutostartStatus {
    autostart_status(
        app.autolaunch()
            .is_enabled()
            .map_err(|error| error.to_string()),
    )
}

fn apply_autostart<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AutostartStatus {
    let operation = if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    };
    autostart_set_status(operation.map_err(|error| error.to_string()), || {
        app.autolaunch()
            .is_enabled()
            .map_err(|error| error.to_string())
    })
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>, quick_check: bool) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    if quick_check {
        let _ = window.emit("quick-check", ());
    }
}

fn handle_close_request<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && close_action(false) == WindowCloseAction::HideToTray
    {
        api.prevent_close();
        let _ = window.hide();
    }
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let initial_autostart = query_autostart(app.handle());

    let title = MenuItemBuilder::with_id("title", "Commercial Preflight")
        .enabled(false)
        .build(app)?;
    let open = MenuItemBuilder::with_id("open", "Open").build(app)?;
    let quick_check = MenuItemBuilder::with_id("quick-check", "Quick Check").build(app)?;
    let autostart = CheckMenuItemBuilder::with_id("autostart", "Start with Windows")
        .checked(initial_autostart.enabled)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Commercial Preflight").build(app)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;
    let third_separator = PredefinedMenuItem::separator(app)?;

    let menu = MenuBuilder::new(app)
        .item(&title)
        .item(&first_separator)
        .item(&open)
        .item(&quick_check)
        .item(&second_separator)
        .item(&autostart)
        .item(&third_separator)
        .item(&quit)
        .build()?;

    let autostart_for_menu = autostart.clone();
    TrayIconBuilder::with_id("commercial-preflight")
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("the bundled application icon is configured"),
        )
        .tooltip("Commercial Preflight")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app, false),
            "quick-check" => show_main_window(app, true),
            "autostart" => {
                let current = query_autostart(app);
                let status = if current.available {
                    apply_autostart(app, !current.enabled)
                } else {
                    current
                };
                let _ = autostart_for_menu.set_checked(status.enabled);
            }
            "quit" if close_action(true) == WindowCloseAction::ExitApplication => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle(), false),
            _ => {}
        })
        .build(app)?;

    app.manage(TrayMenuState {
        autostart_item: autostart,
    });
    Ok(())
}

fn is_background_launch() -> bool {
    std::env::args_os().any(|argument| argument == BACKGROUND_ARGUMENT)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([BACKGROUND_ARGUMENT])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            scan_proposal,
            import_quote,
            import_customer_order,
            get_order_inbox,
            get_exceptions,
            get_order_detail,
            mark_order_reviewed,
            get_quotes,
            set_quote_lifecycle,
            associate_quote,
            get_folder_intake_settings,
            set_folder_intake_settings,
            poll_folder_intake,
            open_source,
            get_bootstrap,
            get_configuration_defaults,
            complete_onboarding,
            update_company,
            list_policies,
            create_policy,
            update_policy,
            set_active_policy,
            archive_policy,
            list_templates,
            create_template,
            update_template,
            set_default_template,
            disable_template,
            get_history,
            get_recent_history,
            clear_history,
            get_management_aggregates,
            get_app_status,
            get_autostart_status,
            set_autostart
        ])
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()?
                .join("commercial-preflight.sqlite3");
            let store = LocalStore::open(database_path)?;
            let folder_watcher = Mutex::new(FolderWatcherState::load(&store));
            app.manage(ProductState {
                store,
                folder_watcher,
            });
            setup_tray(app)?;
            if !is_background_launch() {
                show_main_window(app.handle(), false);
            }
            Ok(())
        })
        .on_window_event(handle_close_request)
        .run(tauri::generate_context!())
        .expect("Commercial Preflight desktop runtime failed");
}
