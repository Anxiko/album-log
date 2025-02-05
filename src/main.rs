use itertools::Itertools;
use regex::Regex;
use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::{env, io};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::hash::Hash;
use std::io::{BufRead, BufReader, Write};
use std::ops::{AddAssign};
use std::str::FromStr;
use std::sync::LazyLock;

static DATE_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*→\s*(.*?)\s*$").unwrap());
static ALBUM_ENTRY_PATTERN: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\s*(.+?)\s*(?:\((\d+)x\))?$").unwrap());
const TOP_ALBUMS: usize = 20usize;
const TOP_ARTISTS: usize = 10usize;
const ENTRY_SEPARATOR: char = '–';
const ARTIST_JOINER: char = '/';

enum ParsedLine {
	Entry(FreqEntry<String>),
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
					ParsedLine::Entry(FreqEntry::new(
						album_match
							.get(2)
							.map(|freq| freq.as_str().parse().expect("Parse album entry frequency"))
							.unwrap_or(1),
						album_match[1].to_owned(),
					))
				})
			})
			.ok_or(())
	}
}

struct AlbumLog {
	entries: HashMap<String, Vec<FreqEntry<String>>>,
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

	fn flattened_album_entries(&self) -> impl Iterator<Item=&FreqEntry<String>> {
		self.entries.values().flatten()
	}
}

#[derive(PartialEq, Eq)]
struct FreqEntry<T: Eq + Ord> {
	freq: u32,
	value: T,
}

impl<T: Eq + Ord> FreqEntry<T> {
	fn new(freq: u32, value: T) -> Self {
		Self { freq, value }
	}
}

impl<T: Eq + Ord> PartialOrd for FreqEntry<T> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl<T: Eq + Ord> Ord for FreqEntry<T> {
	fn cmp(&self, other: &Self) -> Ordering {
		Reverse(self.freq)
			.cmp(&Reverse(other.freq))
			.then_with(|| self.value.cmp(&other.value))
	}
}

impl<T: Eq + Ord + Display> Display for FreqEntry<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} (x{})", self.value, self.freq)
	}
}


struct RankedEntry<T: Ord + Eq> {
	idx: u32,
	rank: u32,
	freq_entry: FreqEntry<T>,
}

impl<T: Ord> RankedEntry<T> {
	fn new(idx: u32, rank: u32, freq_entry: FreqEntry<T>) -> Self {
		Self { idx, rank, freq_entry }
	}

	fn from_freq_entries(freq_entries: impl Iterator<Item=FreqEntry<T>>) -> Vec<RankedEntry<T>> {
		freq_entries
			.sorted()
			.scan(None, |maybe_rank: &mut Option<(u32, u32)>, freq_entry| {
				let entry_rank =
					if let Some((rank, rank_freq)) = *maybe_rank {
						if freq_entry.freq != rank_freq {
							*maybe_rank = Some((rank + 1, freq_entry.freq));
							rank + 1
						} else {
							rank
						}
					} else {
						*maybe_rank = Some((1, freq_entry.freq));
						1
					};
				Some((entry_rank, freq_entry))
			})
			.enumerate()
			.map(|(idx, (rank, freq_entry))| {
				Self::new(idx as u32, rank, freq_entry)
			})
			.collect_vec()
	}
}

impl<T: Ord + Eq + Display> RankedEntry<T> {
	fn to_string(&self, width: usize) -> String {
		format!("#{:0width$} {}. {}", self.idx + 1, self.rank, self.freq_entry, width = width)
	}
}

impl<T: Eq + Ord + Display> Display for RankedEntry<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}| {}. {}", self.idx, self.rank, self.freq_entry)
	}
}

#[derive(Default)]
struct Counter<T: Eq + Hash> {
	counter: HashMap<T, u32>,
}

impl<T: Eq + Ord + Hash> Counter<T> {
	fn new() -> Self {
		Self { counter: HashMap::new() }
	}

	fn add(&mut self, value: T, freq: u32) {
		*self.counter.entry(value).or_default() += freq;
	}

	fn to_freq_entries(self) -> impl Iterator<Item=FreqEntry<T>> {
		self.counter
			.into_iter()
			.map(|(value, freq)| FreqEntry::new(freq, value))
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

fn get_artists(album_entry: &str) -> Result<Vec<String>, ()> {
	let (artists, _) = album_entry.split_once(ENTRY_SEPARATOR).ok_or(())?;
	Ok(artists.split(ARTIST_JOINER).map(|s| s.trim().to_owned()).collect_vec())
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

fn print_top<T: Eq + Ord + Display>(ranked_entries: &[RankedEntry<T>], top: usize, summary: impl Fn(u32, u32) -> String) {
	let total = ranked_entries.iter().map(|entry| entry.freq_entry.freq).sum();
	let unique = ranked_entries.len() as u32;
	let mut digits = if unique > 0 { unique.ilog10() } else { 0 };
	if 10u32.pow(digits) < unique {
		digits += 1;
	}

	let mut iter = ranked_entries.iter().peekable();

	iter
		.by_ref()
		.take(top)
		.for_each(|entry| println!("{}", entry.to_string(digits as usize)));
	println!("{}", summary(unique, total));

	if iter.peek().is_some() {
		let response = loop {
			if let Some(response) = prompt() { break response; }
		};
		if response {
			iter.for_each(|entry| println!("{}", entry.to_string(digits as usize)));
		}
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


	let ranked_entries =
		RankedEntry::from_freq_entries(
			album_freq
				.into_iter()
				.map(|(album, freq)| FreqEntry::new(freq, album))
		);

	print_top(
		&ranked_entries,
		TOP_ALBUMS,
		|unique, total| format!("{unique} albums listed, {total} albums listened"),
	);


	let artist_counter =
		ranked_entries
			.iter()
			.flat_map(|ranked_entry| {
				get_artists(&ranked_entry.freq_entry.value)
					.into_iter()
					.flatten()
					.map(|artist| (artist, ranked_entry.freq_entry.freq))
			})
			.fold(Counter::new(), |mut acc, (artist, freq)| {
				acc.add(artist, freq);
				acc
			});

	let ranked_artists = RankedEntry::from_freq_entries(artist_counter.to_freq_entries());
	print_top(&ranked_artists, TOP_ARTISTS, |unique, total| format!("{unique} artists listed, {total} artists listened"));

	Ok(())
}
