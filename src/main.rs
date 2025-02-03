use itertools::Itertools;
use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::env;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::ops::{AddAssign};
use std::str::FromStr;
use std::sync::LazyLock;

static DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^→\s*(.*?)$\s*").unwrap());
static ALBUM_ENTRY_PATTERN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*(.+?)\s*(?:\((\d+)x\))?$").unwrap());

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
		.for_each(|(rank, (album, freq))| {
			let album_entry = AlbumEntry::new(album, freq);
			println!("{rank}. {album_entry}");
		});

	Ok(())
}
