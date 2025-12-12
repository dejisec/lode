use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    pub fn render<'a>(&self, markdown: &'a str) -> Text<'a> {
        let mut lines: Vec<Line> = Vec::new();
        let mut in_code_block = false;
        let mut code_lang: Option<String> = None;
        let mut code_buffer: Vec<&str> = Vec::new();

        for line in markdown.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End code block - render accumulated code
                    let code = code_buffer.join("\n");
                    let highlighted = self.highlight_code(&code, code_lang.as_deref());
                    lines.extend(highlighted);
                    code_buffer.clear();
                    code_lang = None;
                    in_code_block = false;
                } else {
                    // Start code block
                    in_code_block = true;
                    let lang = line.trim_start_matches('`').trim();
                    code_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
                }
                continue;
            }

            if in_code_block {
                code_buffer.push(line);
                continue;
            }

            lines.push(self.render_line(line));
        }

        // Handle unclosed code block
        if in_code_block && !code_buffer.is_empty() {
            let code = code_buffer.join("\n");
            let highlighted = self.highlight_code(&code, code_lang.as_deref());
            lines.extend(highlighted);
        }

        Text::from(lines)
    }

    fn render_line<'a>(&self, line: &'a str) -> Line<'a> {
        // Headers
        if line.starts_with("######") {
            return self.render_header(line.trim_start_matches('#').trim(), 6);
        }
        if line.starts_with("#####") {
            return self.render_header(line.trim_start_matches('#').trim(), 5);
        }
        if line.starts_with("####") {
            return self.render_header(line.trim_start_matches('#').trim(), 4);
        }
        if line.starts_with("###") {
            return self.render_header(line.trim_start_matches('#').trim(), 3);
        }
        if line.starts_with("##") {
            return self.render_header(line.trim_start_matches('#').trim(), 2);
        }
        if line.starts_with('#') {
            return self.render_header(line.trim_start_matches('#').trim(), 1);
        }

        // Horizontal rule
        if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
            return Line::from(Span::styled(
                "─".repeat(40),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Blockquote
        if line.starts_with('>') {
            let content = line.trim_start_matches('>').trim();
            return Line::from(vec![
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    content.to_string(),
                    Style::default()
                        .fg(Color::Gray)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]);
        }

        // Unordered list
        if line.trim_start().starts_with("- ")
            || line.trim_start().starts_with("* ")
            || line.trim_start().starts_with("+ ")
        {
            let indent = line.len() - line.trim_start().len();
            let content = line.trim_start()[2..].to_string();
            let bullet_indent = " ".repeat(indent);
            return Line::from(vec![
                Span::raw(bullet_indent),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(content),
            ]);
        }

        // Ordered list
        if let Some(rest) = self.try_parse_ordered_list(line) {
            let indent = line.len() - line.trim_start().len();
            let bullet_indent = " ".repeat(indent);
            return Line::from(vec![
                Span::raw(bullet_indent),
                Span::styled(rest.0, Style::default().fg(Color::Cyan)),
                Span::raw(rest.1.to_string()),
            ]);
        }

        // Table row
        if line.contains('|') && line.trim().starts_with('|') {
            return self.render_table_row(line);
        }

        // Regular paragraph with inline formatting
        self.render_inline(line)
    }

    fn render_header(&self, content: &str, level: u8) -> Line<'static> {
        let (color, prefix) = match level {
            1 => (Color::Magenta, "█ "),
            2 => (Color::Cyan, "▓ "),
            3 => (Color::Blue, "▒ "),
            _ => (Color::Gray, "░ "),
        };

        Line::from(vec![
            Span::styled(prefix, Style::default().fg(color)),
            Span::styled(
                content.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ])
    }

    fn render_inline<'a>(&self, text: &'a str) -> Line<'a> {
        let mut spans: Vec<Span> = Vec::new();
        let mut chars = text.char_indices().peekable();
        let mut current_start = 0;

        while let Some((i, c)) = chars.next() {
            match c {
                '`' => {
                    // Inline code
                    if current_start < i {
                        spans.push(Span::raw(&text[current_start..i]));
                    }
                    let code_start = i + 1;
                    let mut code_end = code_start;
                    for (j, ch) in chars.by_ref() {
                        if ch == '`' {
                            code_end = j;
                            break;
                        }
                        code_end = j + ch.len_utf8();
                    }
                    if code_end > code_start {
                        spans.push(Span::styled(
                            text[code_start..code_end].to_string(),
                            Style::default()
                                .fg(Color::Yellow)
                                .bg(Color::Rgb(40, 40, 40)),
                        ));
                    }
                    current_start = code_end + 1;
                }
                '*' | '_' => {
                    // Check for bold or italic
                    if let Some((_, next_c)) = chars.peek()
                        && *next_c == c
                    {
                        // Bold **text** or __text__
                        if current_start < i {
                            spans.push(Span::raw(&text[current_start..i]));
                        }
                        chars.next(); // consume second marker
                        let bold_start = i + 2;
                        let mut bold_end = bold_start;
                        #[allow(clippy::while_let_on_iterator)]
                        while let Some((j, ch)) = chars.next() {
                            if ch == c
                                && let Some((_, next_ch)) = chars.peek()
                                && *next_ch == c
                            {
                                chars.next();
                                bold_end = j;
                                break;
                            }
                            bold_end = j + ch.len_utf8();
                        }
                        if bold_end > bold_start && bold_end <= text.len() {
                            spans.push(Span::styled(
                                text[bold_start..bold_end].to_string(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ));
                        }
                        current_start = (bold_end + 2).min(text.len());
                    } else if chars.peek().is_some() {
                        // Italic *text* or _text_
                        if current_start < i {
                            spans.push(Span::raw(&text[current_start..i]));
                        }
                        let italic_start = i + 1;
                        let mut italic_end = italic_start;
                        for (j, ch) in chars.by_ref() {
                            if ch == c {
                                italic_end = j;
                                break;
                            }
                            italic_end = j + ch.len_utf8();
                        }
                        if italic_end > italic_start && italic_end <= text.len() {
                            spans.push(Span::styled(
                                text[italic_start..italic_end].to_string(),
                                Style::default().add_modifier(Modifier::ITALIC),
                            ));
                        }
                        current_start = (italic_end + 1).min(text.len());
                    }
                }
                '[' => {
                    // Link [text](url)
                    if current_start < i {
                        spans.push(Span::raw(&text[current_start..i]));
                    }
                    let link_text_start = i + 1;
                    let mut link_text_end = link_text_start;
                    let mut found_close = false;
                    for (j, ch) in chars.by_ref() {
                        if ch == ']' {
                            link_text_end = j;
                            found_close = true;
                            break;
                        }
                    }
                    if found_close {
                        if let Some((_, '(')) = chars.peek() {
                            chars.next();
                            let mut url_end = link_text_end + 2;
                            for (j, ch) in chars.by_ref() {
                                if ch == ')' {
                                    url_end = j;
                                    break;
                                }
                            }
                            spans.push(Span::styled(
                                text[link_text_start..link_text_end].to_string(),
                                Style::default()
                                    .fg(Color::Blue)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                            current_start = url_end + 1;
                        } else {
                            spans.push(Span::raw(&text[i..link_text_end + 1]));
                            current_start = link_text_end + 1;
                        }
                    } else {
                        current_start = i;
                    }
                }
                '~' => {
                    // Strikethrough ~~text~~
                    if let Some((_, '~')) = chars.peek() {
                        if current_start < i {
                            spans.push(Span::raw(&text[current_start..i]));
                        }
                        chars.next();
                        let strike_start = i + 2;
                        let mut strike_end = strike_start;
                        #[allow(clippy::while_let_on_iterator)]
                        while let Some((j, ch)) = chars.next() {
                            if ch == '~'
                                && let Some((_, '~')) = chars.peek()
                            {
                                chars.next();
                                strike_end = j;
                                break;
                            }
                            strike_end = j + ch.len_utf8();
                        }
                        if strike_end > strike_start && strike_end <= text.len() {
                            spans.push(Span::styled(
                                text[strike_start..strike_end].to_string(),
                                Style::default().add_modifier(Modifier::CROSSED_OUT),
                            ));
                        }
                        current_start = (strike_end + 2).min(text.len());
                    }
                }
                _ => {}
            }
        }

        if current_start < text.len() {
            spans.push(Span::raw(&text[current_start..]));
        }

        if spans.is_empty() {
            Line::from("")
        } else {
            Line::from(spans)
        }
    }

    fn render_table_row(&self, line: &str) -> Line<'static> {
        let trimmed = line.trim();

        // Check if separator row
        if trimmed
            .chars()
            .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
        {
            return Line::from(Span::styled(
                "─".repeat(trimmed.len().min(60)),
                Style::default().fg(Color::DarkGray),
            ));
        }

        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim())
            .collect();

        let mut spans: Vec<Span> = vec![Span::styled("│", Style::default().fg(Color::DarkGray))];
        for cell in cells {
            spans.push(Span::raw(format!(" {} ", cell)));
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }

        Line::from(spans)
    }

    fn try_parse_ordered_list<'a>(&self, line: &'a str) -> Option<(String, &'a str)> {
        let trimmed = line.trim_start();
        let mut num_end = 0;
        for (i, c) in trimmed.char_indices() {
            if c.is_ascii_digit() {
                num_end = i + 1;
            } else if c == '.' && num_end > 0 && i == num_end {
                if trimmed.get(i + 1..i + 2) == Some(" ") {
                    let number = &trimmed[..num_end];
                    let content = &trimmed[i + 2..];
                    return Some((format!("{}. ", number), content));
                }
            } else {
                break;
            }
        }
        None
    }

    fn highlight_code(&self, code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut lines = Vec::new();

        // Add code block header
        lines.push(Line::from(Span::styled(
            format!("┌─ {} ", lang.unwrap_or("code")),
            Style::default().fg(Color::DarkGray),
        )));

        for line in LinesWithEndings::from(code) {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let mut spans: Vec<Span> =
                vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))];

            for (style, text) in ranges {
                spans.push(Span::styled(
                    text.trim_end_matches('\n').to_string(),
                    syntect_to_ratatui_style(style),
                ));
            }

            lines.push(Line::from(spans));
        }

        // Add code block footer
        lines.push(Line::from(Span::styled(
            "└─────",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    }
}

fn syntect_to_ratatui_style(style: SyntectStyle) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    Style::default().fg(fg)
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}
