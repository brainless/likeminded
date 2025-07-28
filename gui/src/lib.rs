use likeminded_core::{CoreError, RedditPost};

#[derive(Debug, Clone)]
pub enum Message {
    PostClicked(String),
    MarkAsRead(String),
    FilterBySubreddit(String),
    OpenSettings,
}

pub struct App {
    posts: Vec<RedditPost>,
    selected_subreddit: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            posts: Vec::new(),
            selected_subreddit: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Result<(), CoreError> {
        match message {
            Message::PostClicked(post_id) => {
                todo!("Handle post click - open in browser")
            }
            Message::MarkAsRead(post_id) => {
                todo!("Handle mark as read")
            }
            Message::FilterBySubreddit(subreddit) => {
                self.selected_subreddit = Some(subreddit);
                Ok(())
            }
            Message::OpenSettings => {
                todo!("Handle settings navigation")
            }
        }
    }

    pub fn view(&self) -> String {
        todo!("Implement Iced view")
    }
}
