//! Rendering. Each Screen wholly replaces the last, and the input line is
//! present on all of them.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, COMMANDS, Origin, Prompt, Screen, Tone, WordView, argument_hint};
use crate::domain::{Book, NoteState, Sighting};
use crate::search::SearchResult;

const SPLASH: &[&str] = &[
    r"██╗   ██╗ ██████╗  ██████╗ █████╗ ██████╗ ",
    r"██║   ██║██╔═══██╗██╔════╝██╔══██╗██╔══██╗",
    r"██║   ██║██║   ██║██║     ███████║██████╔╝",
    r"╚██╗ ██╔╝██║   ██║██║     ██╔══██║██╔══██╗",
    r" ╚████╔╝ ╚██████╔╝╚██████╗██║  ██║██████╔╝",
    r"  ╚═══╝   ╚═════╝  ╚═════╝╚═╝  ╚═╝╚═════╝ ",
];

const DIM: Style = Style::new().fg(Color::DarkGray);
const HEADING: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);

pub fn draw(app: &App, frame: &mut Frame) {
    let [content, message, input] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    match app.screen() {
        Screen::Home => draw_home(app, frame, content),
        Screen::Search { results, selected } => {
            draw_search(app, frame, content, results, *selected)
        }
        Screen::Word(view) => draw_word(frame, content, view),
        Screen::Library { books, selected } => draw_library(app, frame, content, books, *selected),
        Screen::Help => draw_help(frame, content),
    }

    draw_message(app, frame, message);
    draw_input(app, frame, input);
}

fn paragraph(lines: Vec<Line<'_>>, frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        area.inner(ratatui::layout::Margin::new(2, 1)),
    );
}

fn draw_home(app: &App, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = SPLASH
        .iter()
        .map(|art| Line::from(Span::styled(*art, Style::new().fg(Color::Cyan))))
        .collect();

    lines.push(Line::raw(""));
    lines.push(match app.current_book() {
        Some(book) => Line::from(vec![
            Span::styled("Reading  ", DIM),
            Span::styled(book.name.clone(), Style::new().fg(Color::White)),
        ]),
        None => Line::from(Span::styled(
            "No Book yet — set one with /book <name>",
            Style::new().fg(Color::Yellow),
        )),
    });
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Type to search everything you've captured  ·  /add <word> to capture  ·  /help",
        DIM,
    )));

    paragraph(lines, frame, area);
}

fn draw_search(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    results: &[SearchResult],
    selected: usize,
) {
    let query = app.input().trim();

    if results.is_empty() {
        paragraph(
            vec![
                Line::from(Span::styled(
                    format!("Nothing matches \"{query}\""),
                    HEADING,
                )),
                Line::raw(""),
                Line::from(Span::styled(
                    "You have never captured this one.",
                    Style::new().fg(Color::White),
                )),
            ],
            frame,
            area,
        );
        return;
    }

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "{} matching {}",
                results.len(),
                plural(results.len(), "Word")
            ),
            HEADING,
        )),
        Line::raw(""),
    ];

    let width = results
        .iter()
        .map(|result| result.spelling.chars().count())
        .max()
        .unwrap_or(0)
        .max(12);

    for (index, result) in results.iter().enumerate() {
        let marker = if index == selected { "›" } else { " " };
        let style = if index == selected {
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray)
        };

        let mut spans = vec![
            Span::styled(format!("{marker} "), style),
            Span::styled(format!("{:width$}  ", result.spelling), style),
            Span::styled(format!("{:9}", result.field.label()), DIM),
        ];
        if let Some(excerpt) = &result.excerpt {
            spans.push(Span::styled(format!("  {excerpt}"), DIM));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "↑↓ to choose  ·  Enter to open",
        DIM,
    )));

    paragraph(lines, frame, area);
}

fn draw_word(frame: &mut Frame, area: Rect, view: &WordView) {
    let mut lines = vec![
        Line::from(Span::styled(
            view.word.spelling.clone(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];

    if view.definitions.is_empty() {
        // Say so plainly: a gap in the dictionary, not a failure.
        lines.push(Line::from(Span::styled(
            "No Definition — the bundled dictionary doesn't have this one.",
            Style::new().fg(Color::Yellow),
        )));
    } else {
        for definition in &view.definitions {
            lines.push(Line::from(vec![
                Span::styled(format!("({}) ", definition.part_of_speech), DIM),
                Span::styled(definition.text.clone(), Style::new().fg(Color::White)),
            ]));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!(
            "{} {}",
            view.sightings.len(),
            plural(view.sightings.len(), "Sighting")
        ),
        HEADING,
    )));

    for (index, sighting) in view.sightings.iter().enumerate() {
        let marker = if index == view.selected { "› " } else { "  " };
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(marker, Style::new().fg(Color::White)),
            Span::styled(sighting.captured_on(), DIM),
            Span::styled("  ·  ", DIM),
            Span::styled(sighting.book_name.clone(), Style::new().fg(Color::Green)),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  {}", sighting.sentence),
            Style::new().fg(Color::White),
        )));
        lines.push(note_line(sighting));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!(
            "↑↓ to choose a Sighting  ·  /explain rewrites its Note  ·  {}",
            match view.origin {
                Origin::Search { .. } => "Esc to go back to your search",
                Origin::Home => "Esc to go back",
            }
        ),
        DIM,
    )));

    paragraph(lines, frame, area);
}

/// The Note for one Sighting, labelled so it reads as a second opinion rather
/// than as part of the Definition — and never as an empty space.
fn note_line(sighting: &Sighting) -> Line<'static> {
    let label = Span::styled("  Note  ", Style::new().fg(Color::Magenta));
    let body = match (sighting.note_state, &sighting.note) {
        (NoteState::Ready, Some(note)) => Span::styled(note.clone(), Style::new().fg(Color::Gray)),
        (NoteState::Failed, _) => Span::styled(
            "couldn't be written — /explain to ask again".to_string(),
            Style::new().fg(Color::Yellow),
        ),
        // A Note recorded as ready but missing is a hand-edited database; say
        // pending, which is at least true of what will happen next.
        _ => Span::styled("pending…".to_string(), DIM),
    };
    Line::from(vec![label, body])
}

fn draw_library(app: &App, frame: &mut Frame, area: Rect, books: &[Book], selected: usize) {
    let mut lines = vec![Line::from(Span::styled("Library", HEADING)), Line::raw("")];

    if books.is_empty() {
        lines.push(Line::from(Span::styled(
            "No Books yet — add one with /book <name>.",
            Style::new().fg(Color::White),
        )));
        paragraph(lines, frame, area);
        return;
    }

    let current = app.current_book().map(|book| book.id);
    let width = books
        .iter()
        .map(|book| book.name.chars().count())
        .max()
        .unwrap_or(0);

    for (index, book) in books.iter().enumerate() {
        let reading = Some(book.id) == current;
        let marker = if index == selected { "›" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {:width$}  ", book.name),
                if index == selected {
                    Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(Color::Gray)
                },
            ),
            Span::styled(
                format!("{:>4} {}", book.word_count, plural(book.word_count, "Word")),
                DIM,
            ),
            Span::styled(if reading { "   ← reading" } else { "" }, DIM),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "↑↓ to choose  ·  Enter to start reading it",
        DIM,
    )));

    paragraph(lines, frame, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let mut lines = vec![Line::from(Span::styled("Commands", HEADING)), Line::raw("")];

    let width = COMMANDS
        .iter()
        .map(|command| command.name.len() + command.argument.len() + 1)
        .max()
        .unwrap_or(0);

    for command in COMMANDS {
        let surface = format!("{} {}", command.name, command.argument);
        lines.push(Line::from(vec![
            Span::styled(format!("{surface:width$}  "), Style::new().fg(Color::Cyan)),
            Span::styled(command.help, Style::new().fg(Color::White)),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Anything without a leading slash searches everything you've captured.",
        DIM,
    )));

    paragraph(lines, frame, area);
}

fn draw_message(app: &App, frame: &mut Frame, area: Rect) {
    let Some(message) = app.message() else { return };
    let style = match message.tone {
        Tone::Info => Style::new().fg(Color::Green),
        Tone::Warning => Style::new().fg(Color::Yellow),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(message.text.clone(), style))),
        area.inner(ratatui::layout::Margin::new(2, 0)),
    );
}

fn draw_input(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = match app.prompt().label() {
        Some(label) => vec![Span::styled(
            format!("{label} "),
            Style::new().fg(Color::Yellow),
        )],
        None => vec![Span::styled("› ", Style::new().fg(Color::Cyan))],
    };

    spans.push(Span::styled(
        app.input().to_string(),
        Style::new().fg(Color::White),
    ));

    if matches!(app.prompt(), Prompt::None)
        && let Some(hint) = argument_hint(app.input())
    {
        spans.push(Span::styled(hint, DIM));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        area.inner(ratatui::layout::Margin::new(2, 0)),
    );
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}
