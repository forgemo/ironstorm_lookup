
#![feature(btree_range, collections_bound)]
#![deny(missing_docs)]

//! Overview
//! ---------
//! This library contains the internal data structure used by the ironstrom project
//!
//! Design goals
//! ---------------
//! - Lightning fast auto completion / type ahead lookups (~200 microseconds! per lookup)
//! - Not too much searchable text per entry, e.g: street names for locations or movie titles for movies
//! - High number of possible candidates (multiple gigabytes)
//! - It can be recommended, but must not be rquired to fit the whole data set into physical memory
//! - The LookupTable should use virtual memory and OS level optimization to handle larger data sets
//! - Full text search capability
//! - Optimized for hardly ever changing data sets, e.g.: All streets in a country
//! - No mulithreading if not absolutely required => Buy lookup speed with memory, not processing power!
//! - Optimize for returning a small number of matches, e.g: Find first 10 of 2 million movies that contain 'hero'
//! - Only one dimensional coarse sorting required, e.g: Fantasy books should be returnd before science fiction books
//! - Lazy stream/iterator based lookup implementation
//!
//! Accepted drawbacks
//! ------------------
//! - Creating a `LookupTable` for multiple gigabytes of data can take a few minutes
//! - A `LookupTable` can not be modified, only recreated
//! - No fine granular sorting possible: e.g: by lexicographical order
//!
//! Basic Usage
//! -----
//! 1. Create a custom type for the data you want to seacrh for, e.g.: a `Movie` struct
//! 2. Implement the `Lookup` trait for your custom type.
//! 3. Create an `Iterator` that will iterate over all the elements you would like to put into the `LookupTable`
//! 4. Create a new `LookupTable` by calling `LookupTable::from_iter(myMoviesIterator)`
//! 5. Call `myMoviesLookupTable.find("hero")` to get an lazy 'Iterator' over all matching elements
//!
//! Example
//! -------
//!
//! Let's build a `LookupTable` to find restaurants by name.
//!
//! ```rust
//! use std::iter::FromIterator;
//! use ironstorm_lookup::{LookupTable, Lookup, Bucket};
//!
//! // 1. Create a custom struct representing a restaurant
//! struct Restaurant<'a> {
//!     name: &'a str,
//!     cuisine: &'a str
//! }
//!
//! // 2. Implement the `Lookup` trait for `Restaurant` references
//! impl <'a> Lookup for &'a Restaurant<'a> {
//!
//!     // Make the restaurant name searchable
//!     fn searchable_text(&self) -> String {
//!         self.name.to_string()
//!     }
//!
//!     // Decide, based on cuisine, to which `Bucket` a restaurant belongs.
//!     // `Bucket` is just a type alias for an unsigned integer aka usize.
//!     // Matches in lower buckets will be returned before matches in higher buckets.
//!     fn bucket(&self) -> Bucket {
//!         match self.cuisine {
//!             "italian"   => 0,
//!             "german"    => 0,
//!             "chinese"   => 1,
//!             _           => 5
//!         }
//!     }
//! }
//!
//! // 3. Create some restaurants and the according iterator
//! let restaurants = vec![
//!     Restaurant{name:"India Man", cuisine:"indian"},
//!     Restaurant{name:"Ami Guy", cuisine:"american"},
//!     Restaurant{name:"Italiano Pizza", cuisine:"italian"},
//!     Restaurant{name:"Sushi House", cuisine:"chinese"},
//!     Restaurant{name:"Brezel Hut", cuisine:"german"}
//! ];
//! let iter = restaurants.iter();
//!
//! // 4. Create the `LookupTable`
//! let lookup_table = ironstorm_lookup::LookupTable::from_iter(iter);
//!
//! // 5. Find restaurants containing `i`
//!
//!
//! let mut result_iter = lookup_table.find("i");
//!
//! // two times 'Italiano pizza', because it's in the lowest bucket
//! // two times because it has two lower case `i` in the name
//! assert_eq!(result_iter.next().unwrap().name, "Italiano Pizza");
//! assert_eq!(result_iter.next().unwrap().name, "Italiano Pizza");
//!
//! // 'Sushi House', because it's in the second lowest bucket
//! assert_eq!(result_iter.next().unwrap().name, "Sushi House");
//!
//! // 'Ami Guy' or ' India Man'
//! // They are in the same bucket and there is no order within the same bucket
//! let indian_or_american_1 = result_iter.next().unwrap().name;
//! assert!(indian_or_american_1=="India Man" || indian_or_american_1=="Ami Guy");
//!
//! // The other one of 'Ami Guy' or ' India Man'
//! let indian_or_american_2 = result_iter.next().unwrap().name;
//! assert!(indian_or_american_2=="India Man" || indian_or_american_2=="Ami Guy");
//! assert!(indian_or_american_1 != indian_or_american_2);
//!
//! // No more matches
//! // "Brezel Hut" doesn't contain an "i" and was not part of the result.
//! assert!(result_iter.next().is_none());
//! ```

extern crate suffix;
extern crate itertools;

use suffix::SuffixTable;
use std::collections::{BTreeMap};
use std::collections::Bound::{Included, Unbounded};
use std::iter::FromIterator;


/// Every value that is inserted into the lookup table must be assigned to a bucket.
/// Values, assigned to a lower bucket, will be returned before values from a higher bucket.
/// This bucket mechanism is used instead a full blown sorting algorithm to boost performance.
pub type Bucket = usize;

type TextPosition = usize;

const SEPARATOR: &'static str = "\u{FFFF}";

/// Implement this trait for types that are going be put into a `LookupTable`
pub trait Lookup {

    /// The text that will be looked at when a lookup is executed.
    fn searchable_text(&self) -> String;

    /// The bucket in which this item will be put.
    /// Entries in lower buckets will be returned before entries in higher buckets.
    /// Don't introduce too many buckets per `LookupTable`.
    /// The worst case would be to have one Bucket per LookupTable entry.
    /// `Bucket` is just a type alias for an unsigned integer aka usize.
    fn bucket(&self) -> Bucket;
}

/// This is the actual `LookupTable` that creates the in memory data structure and uses it to perform the lookups.
/// It implements the `FromIterator` trait and its `from_iter(..)` method.
/// To create a new `LookupTable` instance, you first have to create an Iterator over some `Lookup` items.
/// Having that iterator, you can call `LookupTable::from_iter(myLookupItemIterator)``.
pub struct LookupTable<'a, V: 'a>  where V: Lookup{
    suffix_table_map: BTreeMap<Bucket, SuffixTable<'a,'a>>,
    position_map: BTreeMap<(Bucket, TextPosition), V>
}

impl <'a, A: Lookup>FromIterator<A> for LookupTable<'a, A>{

    /// Creates a `LookupTable` from the given Iterator
    fn from_iter<T>(iterator: T) -> Self where T: IntoIterator<Item=A>{
        let mut text_map: BTreeMap<Bucket, String> = BTreeMap::new();
        let mut position_map: BTreeMap<(Bucket, TextPosition), A> = BTreeMap::new();

        for value in iterator {
            let mut text = text_map.entry(value.bucket()).or_insert_with(String::new);
            let pos: TextPosition = text.len();

            text.push_str(&value.searchable_text().as_str());
            text.push_str(SEPARATOR);

            position_map.insert((value.bucket(), pos), value);
        }

        let mut suffix_table_map: BTreeMap<Bucket, SuffixTable> = BTreeMap::new();
        for (bucket, text) in text_map.into_iter(){
            suffix_table_map.insert(bucket, SuffixTable::new(text));
        }

        LookupTable{suffix_table_map: suffix_table_map, position_map: position_map}
    }
}

impl <'a, V>LookupTable<'a, V> where V: Lookup{

    fn get_value_for_position(&self, bucket: Bucket, text_position: TextPosition) -> &V{
        if let Some(value) = self.position_map.range((Unbounded, Included(&(bucket,(text_position as usize))))).rev().next() {
            let (&(_, _), value) = value;
            value
        }else {
            panic!("Could not find at least one value in position map.
                    This must be a bug! Please report it on https://github.com/forgemo/ironstorm_lookup/issues");
        }
    }

    /// Searches for `Lookup` entries with a `serachable_text` that contains the given `search_text`.
    /// If the `search_text` is found multiple times for the same entry, the entry will also be returned multiple times.
    /// If no matches are found, the Iterator will immediately start returning `None`.
    /// Entries in lower buckets will be returned before entries in higher buckets.
    /// The method is case sensitive.
    pub fn find(&'a self, search_text: &'a str) -> Box<Iterator<Item=&V> + 'a> {
        let result_iter = self.suffix_table_map.iter()
        .flat_map(move |(bucket, suffix_table)|{
            suffix_table.positions(&search_text).iter().map(move |text_position|(bucket, text_position))
        })
        .map(move |(bucket, position)|self.get_value_for_position(*bucket, *position as usize));
        return Box::new(result_iter);
    }

    /// Returns the number of values for this `LookupTable`
    pub fn len(&self) -> usize {
        self.position_map.len()
    }

    /// Returns the number of buckets for this `LookupTable`
    pub fn bucket_count(&self) -> usize {
        self.suffix_table_map.len()
    }

}


#[cfg(test)]
mod tests {

    use {Lookup, LookupTable};
    use std::iter::FromIterator;

    impl <'a> Lookup for &'a str {
        fn searchable_text(&self) -> String {
            self.to_string()
        }
        fn bucket(&self) -> usize {
            self.len()
        }
    }

    #[test]
    fn it_works1() {
        let strings = vec!["a","a","a","a","a","a"];
        let t = LookupTable::from_iter(strings.into_iter());
        let len = t.find("a").collect::<Vec<_>>().len();
        assert_eq!(len, 6);
    }

    #[test]
    fn it_works2() {
        let strings = vec!["a","ab","abc","abcd","abcde","abcdef"];
        let t = LookupTable::from_iter(strings.into_iter());
        let mut i = t.find("a");
        assert_eq!(&"a", i.next().unwrap());
        assert_eq!(&"ab", i.next().unwrap());
        assert_eq!(&"abc", i.next().unwrap());
        assert_eq!(&"abcd", i.next().unwrap());
        assert_eq!(&"abcde", i.next().unwrap());
        assert_eq!(&"abcdef", i.next().unwrap());
    }

    #[test]
    fn it_works3() {
        let strings = vec!["ZZZ","ZZ","Z"];
        let t = LookupTable::from_iter(strings.into_iter());
        let mut i = t.find("Z");
        assert_eq!(&"Z", i.next().unwrap());
        assert_eq!(&"ZZ", i.next().unwrap());
        assert_eq!(&"ZZ", i.next().unwrap());
        assert_eq!(&"ZZZ", i.next().unwrap());
        assert_eq!(&"ZZZ", i.next().unwrap());
        assert_eq!(&"ZZZ", i.next().unwrap());
    }

    #[test]
    fn it_works4() {
        let strings = vec!["ZZZZZZZZ","ZZZZZZZZZ"];
        let t = LookupTable::from_iter(strings.into_iter());
        let i = t.find("Z");
        assert_eq!(17, i.count());
    }

    #[test]
    fn it_works5() {
        let strings = vec!["ZZZ","ZZZ","A","ZZZ","B","ZZZ"];
        let t = LookupTable::from_iter(strings.into_iter());
        let i = t.find("A");
        assert_eq!(1, i.count());
        let i = t.find("B");
        assert_eq!(1, i.count());
        let i = t.find("Z");
        assert_eq!(12, i.count());
        let i = t.find("ZZZ");
        assert_eq!(4, i.count());
    }

    #[test]
    fn it_works6() {
        let strings = vec!["A","B","C"];
        let t = LookupTable::from_iter(strings.into_iter());
        let i = t.find("D");
        assert_eq!(0, i.count());
    }
}
