use gui::App;
use iced::{Application, Settings};
use likeminded_core::CoreError;

#[tokio::main]
async fn main() -> Result<(), CoreError> {
    tracing_subscriber::fmt()
        .with_env_filter("likeminded=debug,gui=debug")
        .init();

    tracing::info!("Starting Likeminded - Reddit Post Filter");

    let settings = Settings {
        window: iced::window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            min_size: Some(iced::Size::new(800.0, 600.0)),
            ..Default::default()
        },
        ..Default::default()
    };

    LikemindedApp::run(settings).map_err(|e| {
        tracing::error!("Application error: {}", e);
        CoreError::Configuration(format!("GUI error: {e}"))
    })
}

struct LikemindedApp {
    app: App,
}

impl Application for LikemindedApp {
    type Message = gui::Message;
    type Theme = iced::Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        tracing::info!("Initializing application");
        (Self { app: App::new() }, iced::Command::none())
    }

    fn title(&self) -> String {
        "Likeminded - Reddit Post Filter".to_string()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        if let Err(e) = self.app.update(message) {
            tracing::error!("Update error: {}", e);
        }
        iced::Command::none()
    }

    fn view(&self) -> iced::Element<Self::Message> {
        self.app.view()
    }
}
