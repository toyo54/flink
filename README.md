# flink

A file link manager tool for your PC

flink recreates the web on your local machine. On the web, a page can link to another page by name, and a browser resolves that name to a location. flink does the same for your files: you write `notes.lk` inside any text file, and flink resolves `notes` to a real file path through a table you control.

## The idea

| Web | flink |
|---|---|
| Hyperlink | `name.lk` token inside a text file |
| DNS / URL | The link table (`name -- /absolute/path`) |
| Browser | `flink` |
| Dead link (404) | `valid: false` |

Because links point at names rather than paths, you can move a file and update one line in the table, and every document that mentions it keeps working.

## Installation

Requires Rust 1.79 or newer (`std::path::absolute`). Unix-like systems only for now, since `~` expansion relies on `$HOME`.

```
git clone https://github.com/toyo54/flink.git
cd flink
cargo install --path .
```

## Quick start

```
# register a link
flink --add notes ~/Documents/notes.txt

# mention it in any text file
echo "see notes.lk for details" > todo.txt

# read the file and resolve its links
flink -f todo.txt
```

Output:

```
======= File content =======
see notes.lk for details
============================
link name: notes
associated path: /home/you/Documents/notes.txt
valid: true
============================
```

Follow a link directly:

```
flink -l notes
```

## Usage

```
flink [--tbl TABLE_PATH] COMMAND [ARGUMENTS]
```

| Command | Description |
|---|---|
| `-f`, `--file <file_path>` | Read a file and display its links |
| `-l`, `--link <link_name>` | Display the content of the file a link points to, and its links |
| `--add <link_name> <path>` | Add a link to the table |
| `--remove <link_name>` | Remove a link from the table |
| `-L`, `--list` | List all entries in the table |
| `-h`, `--help` | Show help |

| Option | Description |
|---|---|
| `--tbl <path>` | Use a custom table file. Must be the first argument. Default: `~/.config/flink/global_table.tbl` |

### Link names

A link name must be non-empty, must not start with `#`, and must not contain whitespace. Paths are stored as absolute paths. `~` is expanded when adding.

### Link status

In the output of `-f` and `-l`, each `name.lk` token is reported as:

- `valid: true`: the name is in the table and points to an existing file
- `valid: false`: the name is unknown, or the file no longer exists

## The table format

The table is a plain text file you can edit by hand:

```
# lines starting with '#' are comments
notes -- /home/you/Documents/notes.txt
project -- /home/you/work/my project/readme.md
```

One entry per line as `name -- path`. Paths may contain spaces. Blank lines and comments are ignored. The file and its parent directory are created automatically on the first `--add`.

## Limitations

- Unix-only `~` expansion (uses `$HOME`).
- A link must be a whitespace-separated token ending in `.lk`. Links glued to punctuation, like `notes.lk,` or `(notes.lk)`, are not detected.
- `--tbl` must be the first argument. Extra trailing arguments are ignored.
- A missing table file is treated as an empty table, so a typo in `--tbl` produces "Empty table" rather than an error.
- Concurrent invocations are not supported. `--remove` rewrites the table in place and is not crash-safe.
- Links resolve to files only, not directories.

## License

Licensed under the Apache License, Version 2.0. See the [LICENSE](LICENSE) file for details.
