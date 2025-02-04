use itertools::Itertools;
use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::{env, io};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::ops::{AddAssign};
use std::str::FromStr;
use std::sync::LazyLock;

static DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*→\s*(.*?)\s*$").unwrap());
static ALBUM_ENTRY_PATTERN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*(.+?)\s*(?:\((\d+)x\))?$").unwrap());
const TOP_RANK: usize = 20usize;

#[derive(Debug)]
struct AlbumEntry {
	value: String,
	freq: u32,
}

impl Display for AlbumEntry {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} (x{})", self.value, self.freq)
	}
}

impl AlbumEntry {
	fn new(value: String, freq: u32) -> Self {
		Self { value, freq }
	}
}

enum ParsedLine {
	Entry(AlbumEntry),
	Date(String),
}

impl FromStr for ParsedLine {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		DATE_PATTERN
			.captures(s)
			.map(|date_match| ParsedLine::Date(date_match[1].to_owned()))
			.or_else(|| {
				ALBUM_ENTRY_PATTERN.captures(s).map(|album_match| {
					ParsedLine::Entry(AlbumEntry::new(
						album_match[1].to_owned(),
						album_match
							.get(2)
							.map(|freq| freq.as_str().parse().expect("Parse album entry frequency"))
							.unwrap_or(1),
					))
				})
			})
			.ok_or(())
	}
}

struct AlbumLog {
	entries: HashMap<String, Vec<AlbumEntry>>,
	current: Option<String>,
}

impl AlbumLog {
	fn new() -> Self {
		Self {
			entries: HashMap::new(),
			current: None,
		}
	}

	fn feed_line(&mut self, line: ParsedLine) {
		match line {
			ParsedLine::Date(date) => {
				self.current = Some(date);
			}
			ParsedLine::Entry(entry) => {
				if let Some(current_date) = &self.current {
					self.entries
						.entry(current_date.clone())
						.or_default()
						.push(entry);
				} else {
					// println!("Skipping entry before first date: {entry}")
				}
			}
		}
	}

	fn flattened_album_entries(&self) -> impl Iterator<Item=&AlbumEntry> {
		self.entries.values().flatten()
	}
}

struct RankingEntry {
	idx: usize,
	rank: u32,
	album_entry: AlbumEntry,
}

impl RankingEntry {
	fn print_entry(&self, width: usize) {
		println!("#{:0width$} {}. {}", self.idx + 1, self.rank, self.album_entry, width = width);
	}
}

fn main() -> anyhow::Result<()> {
	match &env::args().collect_vec()[..] {
		[_name, file] => process_file(file),
		[name, ..] => {
			eprintln!("Usage: {name} <file.txt>");
			Ok(())
		}
		_ => unreachable!(),
	}
}

fn prompt() -> Option<bool> {
	print!("See all? [Y/n]: ");
	io::stdout().flush().expect("Flush STDOUT");
	let mut response = String::new();

	io::stdin()
		.read_line(&mut response)
		.expect("Read from STDIN");

	match response.trim().to_ascii_lowercase().as_str() {
		"y" | "" => Some(true),
		"n" => Some(false),
		_ => None,
	}
}

fn process_file(path: &str) -> anyhow::Result<()> {
	let file = File::open(path)?;
	let reader = BufReader::new(file);

	let log = reader
		.lines()
		.map(|line| line.expect("Read line from file"))
		.filter(|line| !line.trim().is_empty())
		.fold(AlbumLog::new(), |mut acc, line| {
			let line: ParsedLine = line
				.parse()
				.unwrap_or_else(|_| panic!("Failed to parse line: {line:?}"));
			acc.feed_line(line);
			acc
		});

	let album_freq =
		log.flattened_album_entries()
			.fold(HashMap::<String, u32>::new(), |mut acc, entry| {
				acc.entry(entry.value.clone())
					.or_default()
					.add_assign(entry.freq);
				acc
			});

	let album_freq = album_freq
		.into_iter()
		.sorted_by_key(|(album, freq)| (Reverse(*freq), album.clone()))
		.collect_vec();

	let total_listenings: u32 = album_freq.iter().map(|(_album, freq)| *freq).sum();
	let total_count = album_freq.len() as u32;

	let mut digits = if total_count > 0 { total_count.ilog10() } else { 0 };
	if 10u32.pow(digits) < total_count {
		digits += 1;
	}

	let ranking_entries =
		album_freq
			.into_iter()
			.scan(None, |maybe_rank: &mut Option<(u32, u32)>, (album, freq)| {
				let album_rank =
					if let Some((rank, rank_freq)) = *maybe_rank {
						if freq != rank_freq {
							*maybe_rank = Some((rank + 1, freq));
							rank + 1
						} else {
							rank
						}
					} else {
						*maybe_rank = Some((1, freq));
						1
					};
				Some((album_rank, (album, freq)))
			})
			.enumerate()
			.map(|(idx, (rank, (album, freq)))| {
				RankingEntry { idx, rank, album_entry: AlbumEntry::new(album, freq) }
			})
			.collect_vec();

	let mut iter = ranking_entries.into_iter().peekable();

	iter
		.by_ref()
		.take(TOP_RANK)
		.for_each(|entry| entry.print_entry(digits as usize));
	println!("{total_count} albums listed, {total_listenings} albums listened");

	if iter.peek().is_some() {
		let response = loop {
			if let Some(response) = prompt() { break response; }
		};
		if response {
			iter.for_each(|entry| entry.print_entry(digits as usize));
		}
	}

	Ok(())
}
