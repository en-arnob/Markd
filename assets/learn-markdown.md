# Learn Markdown

Markdown is a lightweight way to format plain text. This page is itself a
Markdown document rendered by Markd — read it to learn the syntax, then open it
in **Edit Mode** (Settings -> Edit Mode) to see the raw source.

## Headings

Start a line with one to six `#` characters:

```
# Heading 1
## Heading 2
### Heading 3
```

## Emphasis

- `*italic*` or `_italic_` renders as *italic*
- `**bold**` or `__bold__` renders as **bold**
- `~~strikethrough~~` renders as ~~strikethrough~~
- Combine them: `***bold italic***` renders as ***bold italic***

## Lists

Unordered lists use `-`, `*`, or `+`:

- First item
- Second item
    - Nested item
    - Another nested item

Ordered lists use numbers:

1. Step one
2. Step two
3. Step three

Task lists use `- [ ]` and `- [x]`:

- [x] Write some Markdown
- [ ] Render it in Markd

## Links

Wrap link text in brackets and the URL in parentheses:

```
[Visit the Markd repo](https://github.com/en-arnob/markd)
```

[Visit the Markd repo](https://github.com/en-arnob/markd)

## Code

Use single backticks for `inline code`, and triple backticks for a fenced
code block (an optional language label goes after the opening backticks):

```rust
fn main() {
    println!("Hello, Markdown!");
}
```

## Blockquotes

Start a line with `>`:

> Markdown lets you write content that is readable as plain text yet renders
> into clean, formatted documents.

## Tables

Build tables with pipes and dashes:

| Syntax    | Result        |
| --------- | ------------- |
| `*text*`  | *text*        |
| `**text**`| **text**      |
| `` `code` `` | `code`     |

## Horizontal Rule

Three or more dashes on their own line draw a divider:

---

That's the core of Markdown. Happy writing!
