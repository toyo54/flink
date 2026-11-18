use std::{
    borrow::Cow,
    collections::BTreeMap,
    env,
    fmt::{self, Display},
    fs::{self, File, OpenOptions},
    io::{self, Error as IOError, Read, Seek, Write},
    path::{Path, PathBuf},
    process,
};

/// This is the path to the default table location
const TABLE_PATH: &str = "~/.config/flink/global_table.tbl";

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
impl From<IOError> for FlinkError {
    fn from(value: IOError) -> Self {
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
    map: BTreeMap<String, PathBuf>,
}

#[derive(Debug)]
enum TableOperations {
    Add(String, String),
    Remove(String),
    List,
}

#[derive(Debug)]
enum Operations {
    Link(String, String),
    File(String, String),
    Table(TableOperations, String),
}

// ==============================================
// Struct implementation section
// ==============================================

impl<'a> Link<'a> {
    fn new(name: &'a str, path: Cow<'a, Path>, valid: bool) -> Self {
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
        let content = match fs::read_to_string(&expanded) {
            Ok(c) => c,
            // if the file was not found
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };

        // creates the map
        let mut map: BTreeMap<String, PathBuf> = BTreeMap::new();

        // Reads the file
        for (i, line) in content.lines().enumerate() {
            let line = line.trim();

            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            let (name, path) = line.split_once(" -- ").ok_or_else(|| {
                FlinkError::TableError(format!(
                    "line {}: expected `name -- path`, got `{}`",
                    i + 1,
                    line
                ))
            })?;

            map.insert(name.trim().to_string(), PathBuf::from(path));
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

        // check the link name
        if name.is_empty() || name.starts_with('#') || name.contains(char::is_whitespace) {
            return Err(FlinkError::InvalidInput(
                "link name must be non-empty, must not start with '#' and must contain no whitespace".to_string(),
            ));
        }

        // extract the absolute path
        let path = std::path::absolute(expand_tilde(path))?;

        // check the file path
        let Some(path) = path.to_str() else {
            return Err(FlinkError::UnsafePath(format!(
                "The provided path: {} is invalid",
                path.display()
            )));
        };

        if path.contains('\n') {
            return Err(FlinkError::InvalidInput(
                "Path must not contain a newline".to_string(),
            ));
        }

        // create the directory if it doesn't exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        // open (and create) the table
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.path)?;

        if !ends_with_newline(&mut file)? {
            writeln!(file)?; // adds a newline to the end of the file
        }

        // append it to the file and the table
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
    match (path.strip_prefix("~"), env::var_os("HOME")) {
        (Ok(rest), Some(home)) => Cow::Owned(PathBuf::from(home).join(rest)),
        _ => Cow::Borrowed(path),
    }
}

/// Checks whether the file ends with a newline
fn ends_with_newline(file: &mut File) -> io::Result<bool> {
    if file.metadata()?.len() == 0 {
        return Ok(true);
    }

    file.seek(io::SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;

    Ok(last[0] == b'\n')
}

/// This function reads the content from a file located at **path**
fn read_file(path: &Path) -> Result<String, FlinkError> {
    // safety check
    if !path.is_file() {
        return Err(FlinkError::UnsafePath(format!(
            "The provided path: \"{}\" is not a file",
            path.display()
        )));
    }

    Ok(fs::read_to_string(path)?)
}

/// This function returns a list of all the
/// the links found into **slice** that have
/// a match into **table**
fn getlinks<'a>(table: &'a Table, slice: &'a str) -> impl Iterator<Item = Link<'a>> {
    slice.split_whitespace().filter_map(|word| {
        // do not consider ".lk"
        let link_name = word.strip_suffix(".lk")?;

        // check that link_name exists into the table
        let path = match table.get_path(link_name) {
            Some(path) => path,
            None => Path::new(""),
        };

        let path = expand_tilde(path);
        let valid = path.is_file();
        Some(Link::new(link_name, path, valid))
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

    // Check for optional --tbl flag
    let tbl = if args.first().map(|s| s.as_str()) == Some("--tbl") {
        args.remove(0); // Remove "--tbl"
        if args.is_empty() {
            return Err(FlinkError::InvalidInput(
                "Missing table path after --tbl".to_string(),
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
        "-l" | "--link" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing link name for -l command".to_string(),
                ));
            }
            let link_name = args.remove(0);
            Ok(Operations::Link(link_name, tbl))
        }
        "-f" | "--file" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing file path for -f command".to_string(),
                ));
            }
            let file_path = args.remove(0);
            Ok(Operations::File(file_path, tbl))
        }
        "--add" => {
            if args.len() < 2 {
                return Err(FlinkError::InvalidInput(
                    "Missing link name or path for --add command".to_string(),
                ));
            }
            let link_name = args.remove(0);
            let path = args.remove(0);
            Ok(Operations::Table(
                TableOperations::Add(link_name, path),
                tbl,
            ))
        }
        "--remove" => {
            if args.is_empty() {
                return Err(FlinkError::InvalidInput(
                    "Missing link name for --remove command".to_string(),
                ));
            }
            let link_name = args.remove(0);

            Ok(Operations::Table(TableOperations::Remove(link_name), tbl))
        }
        "--list" | "-L" => Ok(Operations::Table(TableOperations::List, tbl)),
        "-h" | "--help" => show_info(),
        _ => Err(FlinkError::InvalidInput(format!(
            "Unknown command: {}",
            command
        ))),
    }
}

/// This function prints the help message to the user
fn show_info() -> ! {
    println!("flink - File Link Manager");
    println!();
    println!("USAGE:");
    println!("    flink [--tbl TABLE_PATH] COMMAND [ARGUMENTS]");
    println!();
    println!("OPTIONS:");
    println!("    --tbl <path>    Specify a custom table file path");
    println!("                   (default: ~/.config/flink/global_table.tbl)");
    println!();
    println!("COMMANDS:");
    println!("    --file | -f <file_path>              Read a file and display its links");
    println!("    --link | -l <link_name>              Display file content from a link name");
    println!("    --add <link_name> <path>             Adds a new link to the table");
    println!("    --remove <link_name>                 Remove a link from the table");
    println!("    --list | -L                          List all the entries found inside a table");

    process::exit(0);
}

// ==============================================
// Runner
// ==============================================

/// Matches and runs the correct command
fn run() -> Result<(), FlinkError> {
    let op = get_op()?;

    match op {
        Operations::File(val, tbl) => handle_file(&val, &tbl),
        Operations::Link(val, tbl) => handle_link(&val, &tbl),
        Operations::Table(tbl_op, tbl_path) => handle_table(tbl_op, &tbl_path),
    }
}

/// Handles the "file" command
fn handle_file(val: &str, tbl: &str) -> Result<(), FlinkError> {
    let table = Table::read(Path::new(tbl))?;
    let path = expand_tilde(Path::new(val));
    let fc = read_file(&path)?;

    let links = getlinks(&table, &fc);
    show_output(&fc, links);

    Ok(())
}

/// Handles the link command
fn handle_link(val: &str, tbl: &str) -> Result<(), FlinkError> {
    let table = Table::read(Path::new(tbl))?;

    let path = table.get_path(val).ok_or_else(|| {
        FlinkError::TableError(format!("Unable to find a path for link: {}", val))
    })?;

    let path = Path::new(&path);
    let content = read_file(&expand_tilde(path))?;

    let links = getlinks(&table, &content);
    show_output(&content, links);

    Ok(())
}

/// Handles table related operations
fn handle_table(tbl_op: TableOperations, tbl_path: &str) -> Result<(), FlinkError> {
    let mut table = Table::read(Path::new(tbl_path))?;

    match tbl_op {
        TableOperations::Add(name, path) => {
            table.add_link(name.as_str(), Path::new(&path))?;
            println!("Link: \"{}\" added successfully!", name);
        }
        TableOperations::Remove(name) => {
            table.remove_link(name.as_str())?;
            println!("Link: \"{}\" removed successfully!", name);
        }
        TableOperations::List => {
            if table.map.is_empty() {
                println!("Empty table: {}", table.path.display());
                return Ok(());
            }

            for (k, v) in &table.map {
                println!("LINK NAME: {} => FILE PATH: {}", k, v.display());
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
    // from the run function, which is the core dispatcher
    // of the program
    if let Err(err) = run() {
        eprintln!("Error: \n{}", err);
        process::exit(1);
    }
}
