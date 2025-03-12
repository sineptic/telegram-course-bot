use std::path::PathBuf;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TelegramInteraction {
    OneOf(Vec<String>),
    Text(String),
    UserInput,
    Image(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionElement {
    Text(String),
    Image(PathBuf),
}
impl From<QuestionElement> for TelegramInteraction {
    fn from(element: QuestionElement) -> Self {
        match element {
            QuestionElement::Text(text) => TelegramInteraction::Text(text),
            QuestionElement::Image(image) => TelegramInteraction::Image(image),
        }
    }
}
impl QuestionElement {
    pub fn from_str(input: &str) -> Self {
        assert!(input.lines().count() == 1);
        assert!(!input.trim().is_empty());

        let input = input.trim();
        match input.as_bytes()[0] {
            b'!' => {
                let error_msg = "Image should have this syntax: ![path_to_image]";
                let input = input.strip_prefix("![").expect(error_msg);
                let input = input.strip_suffix("]").expect(error_msg);
                QuestionElement::Image(PathBuf::from(input))
            }
            _ => QuestionElement::Text(input.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub question: Vec<QuestionElement>,
    pub options: Vec<String>,
    pub answer: usize,
}
impl Task {
    pub fn correct_answer(&self) -> &str {
        &self.options[self.answer]
    }
    pub fn interactions(&self) -> Vec<TelegramInteraction> {
        let mut interactions = Vec::new();
        for element in &self.question {
            interactions.push(element.clone().into());
        }
        interactions.push(TelegramInteraction::OneOf(self.options.clone()));
        interactions
    }
}

impl Task {
    pub fn from_str(input: &str) -> Self {
        assert!(!input.trim().is_empty());
        let input = input.trim();
        let lines = input.lines().map(|x| x.trim()).collect::<Vec<_>>();
        let mut question = Vec::new();
        let mut i = 0;
        let err_msg = "Task should follow this syntax:
...
input
...
           <- empty line
* correct option
- options
...
";
        loop {
            if i >= lines.len() {
                panic!("{err_msg}. No 'options' provided")
            }
            if lines[i].is_empty() {
                if i == 0 {
                    panic!("{err_msg}. No 'input' provided")
                }
                i += 1;
                break;
            }
            question.push(QuestionElement::from_str(lines[i]));
            i += 1;
        }

        let mut options = Vec::new();
        if i >= lines.len() {
            panic!("{err_msg}. No 'options' provided")
        }
        assert!(
            lines[i].starts_with("* "),
            "{err_msg}. First option should be correct and starts with '* '"
        );
        options.push(
            lines[i]
                .strip_prefix("* ")
                .expect("Implementation issue")
                .to_owned(),
        );
        i += 1;

        assert_ne!(
            i,
            lines.len(),
            "{err_msg}. Should contain at least one non correct option"
        );
        while i < lines.len() {
            options.push(
                lines[i]
                    .strip_prefix("- ")
                    .unwrap_or_else(|| {
                        panic!("{err_msg}. Option at line {} is missing '- ' prefix", i + 1)
                    })
                    .to_owned(),
            );
            i += 1;
        }

        Task {
            question,
            options,
            answer: 0,
        }
    }
}
