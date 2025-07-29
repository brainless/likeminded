use iced::widget::{button, column, container, text, Column};
use iced::{Element, Length, Theme};
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
            Message::PostClicked(_post_id) => {
                todo!("Handle post click - open in browser")
            }
            Message::MarkAsRead(_post_id) => {
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

    pub fn view(&self) -> Element<Message, Theme> {
        let title: Element<Message, Theme> =
            text("Likeminded - Reddit Post Filter").size(24).into();

        let content: Element<Message, Theme> = if self.posts.is_empty() {
            column![
                text("No posts available").size(16),
                text("Connect to Reddit to start filtering posts").size(14)
            ]
            .spacing(10)
            .into()
        } else {
            let mut post_list = Column::new().spacing(10);
            for post in &self.posts {
                let post_element: Element<Message, Theme> = container(
                    column![
                        text(&post.title).size(16),
                        text(format!("r/{}", post.subreddit)).size(12),
                        button("Mark as Read").on_press(Message::MarkAsRead(post.id.clone()))
                    ]
                    .spacing(5),
                )
                .padding(10)
                .into();
                post_list = post_list.push(post_element);
            }
            post_list.into()
        };

        let main_content: Element<Message, Theme> = column![title, container(content).padding(20)]
            .spacing(20)
            .into();

        container(main_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }
}
