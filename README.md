Currently, this project doesn't support you to share courses, but this feature is going to be implemented.

# Telegram Bot for Spaced Repetition Learning

This project is a Telegram bot designed to help teachers. It allows students to review educational content using a spaced repetition system. The bot presents users with questions and tracks their progress, scheduling revisions at optimal intervals to improve long-term retention.

## Features

- **Spaced Repetition Learning**
  The bot uses the FSRS (Free Spaced Repetition Scheduler) algorithm to schedule card revisions.
- **Course Structure as a Graph**
  The learning material is structured as a directed acyclic graph (DAG), where nodes represent concepts and edges represent dependencies. This allows for a structured learning path.
- **Customizable Content**:
  Course content, including the graph structure and the questions (cards), can be easily customized by editing simple text files.
- **Progress Visualization**:
  The bot can generate and display a visual representation of the course graph, with nodes colored according to the user's progress.

## How to Use

1.  **Set up the bot**:
    - Clone the repository.
    - Create a `.env` file and add your Telegram bot token: `TELOXIDE_TOKEN=your_token_here`.
      You can create it using BotFather (@Father558_Bot).
    - Make sure you have `graphviz` installed. (a tool used to generate graph images)

2.  **Run the bot**:
    ```bash
    cargo run --release
    ```

3.  **Customize the course**:
    Run `/change_course_graph` and `/change_deque` in the bot.
    You can use new version only after updating both course graph and deque.
    If it doesn't work, check `/view_course_errors`.

4.  **Interact with the bot**:
    Run `/help` command to view available commands.
    Run `/card` to complete a task.

    Currently, progress tracking is disabled to simplify exploration.

## Continuous Integration

The project has a CI pipeline set up with GitHub Actions. The pipeline checks for formatting, lints the code, and builds the project on every push and pull request.

You can download binary from Actions tab on GitHub. (Make sure you are logged in!)
