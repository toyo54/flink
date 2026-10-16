use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    fmt::{self, Display},
    fs::{self, OpenOptions},
    io::{self, Error as IOError, Write},
    path::{Path, PathBuf},
    process,
};

/// This is the path to the default table location
const TABLE_PATH: &str = "~/.config/flnk/global_table.tbl";

// ==============================================
// Error section
// ==============================================
#[derive(Debug)]
enum FlinkError {
    FileError(IOError),
    TableError(String),
    UnsafePath(String),
    InvalidInput(String),
}

impl fmt::Display for FlinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlinkError::FileError(error) => {
                write!(f, "A file error happened: {}", error)
            }
            FlinkError::TableError(msg) => {
                write!(f, "An error occurred with the table:\n{}", msg)
            }
            FlinkError::UnsafePath(path) => {
                write!(f, "The provided path is unsafe: {}", path)
            }
            FlinkError::InvalidInput(msg) => {
                write!(f, "Input error: {}", msg)
            }
        }
    }
}

impl std::error::Error for FlinkError {}
impl From<io::Error> for FlinkError {
    fn from(value: io::Error) -> Self {
        FlinkError::FileError(value)
    }
}

// =============================================
// Data section
// =============================================

struct Link<'a> {
    name: &'a str,
    path: Cow<'a, Path>,
    valid: bool,
}

struct Table {
    path: PathBuf,
    map: HashMap<String, PathBuf>,
}

#[derive(Debug)]
enum TableOperations {
    Add(String, String),
    Remove(String),
}

#[derive(Debug)]
enum Operations {
    Link(String, String),
    File(String, String),
    Table(TableOperations, String),
}

// ==============================================
// Struct implementations section
// ==============================================

impl<'a> Link<'a> {
    fn from(name: &'a str, path: Cow<'a, Path>, valid: bool) -> Self {
        Link { name, path, valid }
    }
}

impl Display for Link<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.to_str().unwrap_or_default();

        write!(
            f,
            "link name: {}\nassociated path: {}\nvalid: {}",
            self.name, path, self.valid
        )
    }
}

impl Table {
    /// This method opens the provided **table_path** and returns an instance of it
    fn read(user_path: &Path) -> Result<Table, FlinkError> {
        let expanded = expand_tilde(user_path);

        // opens the file and reads the content
        let content = fs::read_to_string(&expanded)?;

        // creates the map
        let mut map: HashMap<String, PathBuf> = HashMap::new();

        // Reads the file
        for line in content.lines() {
            let line = line.trim();

            // check for comment or empty line
            if line.starts_with("#") || line.is_empty() {
                continue;
            }

            let mut split = line.split_whitespace();

            let link_name = split.next().ok_or(FlinkError::TableError(
                "Invalid table structure".to_string(),
            ))?;

            // check for separator
            if split.next() != Some("--") {
                return Err(FlinkError::TableError(
                    "Invalid table structure".to_string(),
                ));
            }

            let path = split.next().ok_or(FlinkError::TableError(
                "Invalid table structure".to_string(),
            ))?;

            map.insert(link_name.to_string(), PathBuf::from(path));
        }

        Ok(Table {
            path: expanded.to_path_buf(),
            map,
        })
    }

    /// This method obtains the path associated to **link_name**
    ///
    /// NOTE:
    /// The obtained path may not be expanded and
    /// it is the caller's duty to correctly handle
    /// this scenario
    fn get_path(&self, link_name: &str) -> Option<&PathBuf> {
        self.map.get(link_name)
    }

    ///This functions returns true if the provided
    ///link name is present inside the table
    fn is_present(&self, link_name: &str) -> bool {
        self.map.contains_key(link_name)
    }

    fn add_link(&mut self, name: &str, path: &Path) -> Result<(), FlinkError> {
        if self.is_present(name) {
            return Err(FlinkError::TableError(format!(
                "Link: {} is already inside the table",
                name
            )));
        }

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)?;

        let Some(path) = path.to_str() else {
            return Err(FlinkError::UnsafePath(format!(
                "The provided path: {} is invalid",
                path.display()
            )));
        };

        writeln!(file, "{} -- {}", name, path)?;
        self.map.insert(name.to_string(), PathBuf::from(path));
        Ok(())
    }

    fn remove_link(&mut self, name: &str) -> Result<(), FlinkError> {
        if !self.is_present(name) {
            return Err(FlinkError::TableError(format!(
                "Link: \"{}\" is not in the table",
                name
            )));
        }

        // TODO: remove the line

        let content = std::fs::read_to_string(&self.path)?;
        let content: String = content
            .split_inclusive('\n')
            .filter(|l| l.split_whitespace().next() != Some(name))
            .collect();

        std::fs::write(&self.path, content)?;
        self.map.remove(name);

        Ok(())
    }
}

// ==============================================
// Core functions
// ==============================================

/// This function expands **path** if it starts with "~"
fn expand_tilde(path: &Path) -> Cow<'_, Path> {
    let path_str = path.to_string_lossy();
    let home = env::var_os("HOME");

    let Some(home_val) = home else {
        return Cow::Borrowed(path);
    };

    let home = PathBuf::from(home_val);
    if path_str == "~" {
        return Cow::Owned(home);
    }

    if let Some(suffix) = path_str.strip_prefix("~/") {
        return Cow::Owned(home.join(suffix));
    }

    Cow::Borrowed(path)
}

/// This function reads the content from a file located at **path**
fn read_file(path: &Path) -> Result<String, FlinkError> {
    // safety check
    if !path.is_file() {
        return Err(FlinkError::UnsafePath(
            "The provided path is unsafe".to_string(),
        ));
    }

    Ok(fs::read_to_string(path)?)
}

/// This function returns a list of all the
/// the links found into **str** that have
/// a match into **table**
fn getlinks<'a>(table: &'a Table, str: &'a str) -> impl Iterator<Item = Link<'a>> {
    str.split_whitespace().filter_map(|word| {
        // do not consider ".lk"
        let link_name = word.strip_suffix(".lk")?;

        // check that link_name exists into the table
        let path = match table.get_path(link_name) {
            Some(path) => path,
            None => Path::new(""),
        };

        let path = expand_tilde(path);
        let valid = path.exists();
        Some(Link::from(link_name, path, valid))
    })
}

// ===================================================
// CLI logic section
// ===================================================
fn show_output<'a>(content: &str, links: impl Iterator<Item = Link<'a>>) {
    println!("======= File content =======");
    println!("{content}");
    println!("============================");

    links.for_each(|link| {
        println!("{link}");
        println!("============================");
    });
}
fn get_op() -> Result<Operations, FlinkError> {
    let mut args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        return Err(FlinkError::InvalidInput(
            "No arguments provided".to_string(),
        ));
    }

    // Check for optional -tbl flag
    let tbl = if args.first().map(|s| s.as_str()) == Some("-tbl") {
        args.remove(0); // Remove "-tbl"
        if args.is_empty() {
            return Err(FlinkError::InvalidInput(
                "Missing table path after -tbl".to_string(),
            ));
        }
        args.remove(0) // Remove and return the table path
    } else {
        String::from(TABLE_PATH)
    };

    // Now parse the actual command
    if args.is_empty() {
        return Err(FlinkError::InvalidInput("Missing command".to_string()));
    }

    let command = args.remove(0);

    match command.as_str() {
        "-l" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing link name for -l command".to_string(),
                ));
            }
            let link_name = args.remove(0);
            Ok(Operations::Link(link_name, tbl))
        }
        "-f" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing file path for -f command".to_string(),
                ));
            }
            let file_path = args.remove(0);
            Ok(Operations::File(file_path, tbl))
        }
        "-add" => {
            if args.len() < 2 {
                return Err(FlinkError::InvalidInput(
                    "Missing link name or path for -add command".to_string(),
                ));
            }
            let link_name = args.remove(0);
            let path = args.remove(0);
            Ok(Operations::Table(
                TableOperations::Add(link_name, path),
                tbl,
            ))
        }
        "-rm" | "-remove" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing link name for -rm command".to_string(),
                ));
            }
            let link_name = args.remove(0);
            // Remove doesn't need the second String in the enum, but it's defined that way
            // so we'll pass an empty string
            Ok(Operations::Table(TableOperations::Remove(link_name), tbl))
        }
        _ => Err(FlinkError::InvalidInput(format!(
            "Unknown command: {}. Use -f, -l, -add, or -rm",
            command
        ))),
    }
}

/// This function prints the help message to the user
fn show_info() -> ! {
    println!("flink - File Link Manager");
    println!();
    println!("USAGE:");
    println!("    flink [-tbl TABLE_PATH] COMMAND [ARGUMENTS]");
    println!();
    println!("OPTIONS:");
    println!("    -tbl <path>    Specify a custom table file path");
    println!("                   (default: ~/.config/flnk/global_table.tbl)");
    println!();
    println!("COMMANDS:");
    println!("    -f <file_path>              Read a file and display its links");
    println!("    -l <link_name>              Display file content from a link name");
    println!("    -add <link_name> <path>     Add a new link to the table");
    println!("    -rm <link_name>             Remove a link from the table");
    println!("    -remove <link_name>         Remove a link from the table (alias)");
    println!();
    println!("EXAMPLES:");
    println!("    flink -f ./my_note.txt");
    println!("    flink -l note_personali");
    println!("    flink -add mylink ~/documents/notes.txt");
    println!("    flink -tbl ./custom.tbl -add work ~/work/todo.txt");
    println!("    flink -rm mylink");

    process::exit(2);
}

// ==============================================
// Runner
// ==============================================

/// This function is responsible
/// for executing the right command
/// provided by the user
fn run() -> Result<(), FlinkError> {
    // Obtain the operation
    let op = match get_op() {
        Ok(op) => op,
        Err(e) => {
            eprintln!("Error: {e}\n");
            show_info();
        }
    };

    match op {
        Operations::File(val, tbl) => {
            let path = Path::new(&tbl);
            let table = Table::read(path)?;

            // file
            let path = expand_tilde(Path::new(&val));
            let fc = read_file(&path)?;

            // output
            let links = getlinks(&table, &fc);
            show_output(&fc, links);
        }
        Operations::Link(val, tbl) => {
            // table
            let path = Path::new(&tbl);
            let table = Table::read(path)?;

            // obtains path from a link name
            let path = table.get_path(&val).ok_or(FlinkError::TableError(format!(
                "Unable to find a path for link: {}",
                val
            )))?;

            // read the file
            let path = Path::new(&path);
            let content = read_file(&expand_tilde(path))?;

            // output
            let links = getlinks(&table, &content);
            show_output(&content, links);
        }
        Operations::Table(tbl_op, tbl_path) => {
            // table
            let path = Path::new(&tbl_path);
            let mut table = Table::read(path)?;

            match tbl_op {
                TableOperations::Add(name, path) => {
                    table.add_link(name.as_str(), Path::new(&path))?;
                    println!("Link: \"{}\" added successfully!", name);
                }
                TableOperations::Remove(name) => {
                    table.remove_link(name.as_str())?;
                    println!("Link: \"{}\" removed successfully!", name);
                }
            }
        }
    }

    Ok(())
}

// ==============================================
// Main
// ==============================================

fn main() {
    // The main function is only used to check the error returned
    // from the run function, wich is the core dispatcher
    // of the program
    if let Err(err) = run() {
        eprintln!("Error: \n{}", err);
        process::exit(1);
    }
}

// ===============================================
// TEST
// ===============================================

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn from_file() {
        // table
        let path = Path::new(TABLE_PATH);
        let table = Table::read(path).unwrap();

        // file
        let path = Path::new("./my_note.txt");
        let fc = read_file(path).unwrap();

        // output
        let links = getlinks(&table, &fc);
        show_output(&fc, links);
    }

    #[test]
    fn from_link() {
        // table
        let path = Path::new(TABLE_PATH);
        let table = Table::read(path).unwrap();

        // obtain path from a link name and read the file
        let ln = "note_personali";
        let path = table.get_path(ln).unwrap();
        let path = Path::new(&path);
        let path = expand_tilde(path);
        dbg!(&path);
        let content = read_file(&path).unwrap();

        // output
        let links = getlinks(&table, &content);
        show_output(&content, links);
    }
}
