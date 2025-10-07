pub enum NewEventPath {
    NewEvent,
}

impl NewEventPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            NewEventPath::NewEvent => "/new_event",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            NewEventPath::NewEvent => "/new_event",
        }
    }
}
