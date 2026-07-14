use super::schema::ConversationType;

/// Classify the type of conversation from text input.
/// Rule-based — no ML model required.
pub fn classify(text: &str) -> ConversationType {
    let lower = text.to_lowercase().trim().to_string();

    // Questions
    if lower.ends_with('?')
        || lower.starts_with("what")
        || lower.starts_with("how")
        || lower.starts_with("why")
        || lower.starts_with("when")
        || lower.starts_with("where")
        || lower.starts_with("who")
        || lower.starts_with("which")
        || lower.starts_with("is there")
        || lower.starts_with("are there")
        || lower.starts_with("can i")
        || lower.starts_with("should i")
    {
        return ConversationType::Question;
    }

    // Code requests
    if lower.contains("write code")
        || lower.contains("write a function")
        || lower.contains("write a class")
        || lower.contains("implement")
        || lower.contains("debug this")
        || lower.contains("fix this bug")
        || lower.contains("refactor")
        || lower.contains("code review")
        || lower.contains("```")
    {
        return ConversationType::Code;
    }

    // Tasks
    if lower.starts_with("create")
        || lower.starts_with("build")
        || lower.starts_with("make")
        || lower.starts_with("set up")
        || lower.starts_with("configure")
        || lower.starts_with("deploy")
        || lower.starts_with("install")
        || lower.starts_with("add")
        || lower.starts_with("update")
        || lower.starts_with("change")
        || lower.starts_with("delete")
        || lower.starts_with("remove")
    {
        return ConversationType::Task;
    }

    // Explanation requests
    if lower.starts_with("explain")
        || lower.starts_with("tell me about")
        || lower.starts_with("describe")
        || lower.starts_with("what is")
        || lower.starts_with("how does")
        || lower.starts_with("why does")
    {
        return ConversationType::Explanation;
    }

    // Problem reports
    if lower.contains("error")
        || lower.contains("not working")
        || lower.contains("broken")
        || lower.contains("fails")
        || lower.contains("crash")
        || lower.contains("issue")
        || lower.contains("problem")
        || lower.contains("bug")
    {
        return ConversationType::Problem;
    }

    // Brainstorm
    if lower.contains("what if")
        || lower.contains("what about")
        || lower.contains("idea")
        || lower.contains("brainstorm")
        || lower.contains("explore")
        || lower.contains("think about")
        || lower.contains("consider")
    {
        return ConversationType::Brainstorm;
    }

    ConversationType::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_questions() {
        assert_eq!(classify("What is Rust?"), ConversationType::Question);
        assert_eq!(classify("how do I deploy this"), ConversationType::Question);
        assert_eq!(classify("is this thread safe?"), ConversationType::Question);
    }

    #[test]
    fn classifies_code_requests() {
        assert_eq!(
            classify("please implement a binary search"),
            ConversationType::Code
        );
        assert_eq!(classify("refactor the auth module"), ConversationType::Code);
    }

    #[test]
    fn classifies_tasks() {
        assert_eq!(classify("create a new config file"), ConversationType::Task);
        assert_eq!(classify("deploy the staging build"), ConversationType::Task);
    }

    #[test]
    fn classifies_explanations() {
        assert_eq!(
            classify("explain the borrow checker"),
            ConversationType::Explanation
        );
    }

    #[test]
    fn classifies_problems() {
        assert_eq!(
            classify("the login page is broken"),
            ConversationType::Problem
        );
    }

    #[test]
    fn classifies_brainstorm() {
        assert_eq!(
            classify("let's brainstorm names for the product"),
            ConversationType::Brainstorm
        );
    }

    #[test]
    fn falls_back_to_other() {
        assert_eq!(classify("good morning"), ConversationType::Other);
    }
}
