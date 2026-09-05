use gpui::*;

use crate::shared::actions::*;
use crate::ui::WorkspaceView;

pub fn run() {
    let app = Application::new();

    app.run(|cx| {
        cx.bind_keys([
            KeyBinding::new("f5", RefreshNow, Some("Omniview")),
            KeyBinding::new("ctrl-1", ShowProcesses, Some("Omniview")),
            KeyBinding::new("ctrl-2", ShowPorts, Some("Omniview")),
            KeyBinding::new("ctrl-3", ShowFiles, Some("Omniview")),
            KeyBinding::new("ctrl-4", ShowDns, Some("Omniview")),
            KeyBinding::new("ctrl-5", ShowProxy, Some("Omniview")),
            KeyBinding::new("ctrl-q", QuitApp, Some("Omniview")),
        ]);

        cx.on_action(|_: &QuitApp, cx| {
            cx.quit();
        });

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(80.0), px(60.0)),
                        size: size(px(1280.0), px(800.0)),
                    })),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Omniview".into()),
                        appears_transparent: false,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_window, cx| cx.new(|cx| WorkspaceView::new(cx)),
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
