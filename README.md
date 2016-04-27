# ironstorm_lookup

[![Build Status](https://travis-ci.org/forgemo/ironstorm_lookup.svg?branch=master)](https://travis-ci.org/forgemo/ironstorm_lookup) [![](http://meritbadge.herokuapp.com/ironstorm_lookup)](https://crates.io/crates/ironstorm_lookup) [![license: Apache2+MIT](https://img.shields.io/badge/license-Apache2%2BMIT-brightgreen.svg)](https://github.com/forgemo/ironstorm_lookup/blob/master/README.md#license)
![rurst: nightly](https://img.shields.io/badge/rust-nightly-orange.svg)

## Overview

This library contains the internal data structure used by the ironstrom project

To learn more about ironstorm_lookup, read this README.md and the [Crate Documentation](http://forgemo.github.io/docs/ironstorm_lookup/ironstorm_lookup)

It compiles only with the nightly version of rust due tu usage of unstable features.

## Design goals

- Lightning fast auto completion / type ahead lookups (~200 microseconds! per lookup)
- Not too much searchable text per entry, e.g: street names for locations or movie titles for movies
- High number of possible candidates (multiple gigabytes)
- It can be recommended, but must not be rquired to fit the whole data set into physical memory
- The LookupTable should use virtual memory and OS level optimization to handle larger data sets
- Full text search capability
- Optimized for hardly ever changing data sets, e.g.: All streets in a country
- No mulithreading if not absolutely required => Buy lookup speed with memory, not processing power!
- Optimize for returning a small number of matches, e.g: Find first 10 of 2 million movies that contain 'hero'
- Only one dimensional coarse sorting required, e.g: Fantasy books should be returnd before science fiction books
- Lazy stream/iterator based lookup implementation

## Accepted drawbacks

- Creating a `LookupTable` for multiple gigabytes of data can take a few minutes
- A `LookupTable` can not be modified, only recreated
- No fine granular sorting possible: e.g: by lexicographical order

## Basic Usage

1. Create a custom type for the data you want to seacrh for, e.g.: a `Movie` struct
2. Implement the `Lookup` trait for your custom type.
3. Create an `Iterator` that will iterate over all the elements you would like to put into the `LookupTable`
4. Create a new `LookupTable` by calling `LookupTable::from_iter(myMoviesIterator)`
5. Call `myMoviesLookupTable.find("hero")` to get an lazy 'Iterator' over all matching elements

## Example

Let's build a `LookupTable` to find restaurants by name.

```rust
use std::iter::FromIterator;
use ironstorm_lookup::{LookupTable, Lookup, Bucket};

// 1\. Create a custom struct representing a restaurant
struct Restaurant<'a> {
    name: &'a str,
    cuisine: &'a str
}

// 2\. Implement the `Lookup` trait for `Restaurant` references
impl <'a> Lookup for &'a Restaurant<'a> {
    // Make the restaurant name searchable
    fn searchable_text(&self) -> String {
        self.name.to_string()
    }
    // Decide, based on cuisine, to which `Bucket` a restaurant belongs.
    // `Bucket` is just a type alias for an unsigned integer aka usize.
    // Matches in lower buckets will be returned before matches in higher buckets.
    fn bucket(&self) -> Bucket {
        match self.cuisine {
            "italian"   => 0,
            "german"    => 0,
            "chinese"   => 1,
            _           => 5
        }
    }
}

// 3\. Create some restaurants and the according iterator
let restaurants = vec![
    Restaurant{name:"India Man", cuisine:"indian"},
    Restaurant{name:"Ami Guy", cuisine:"american"},
    Restaurant{name:"Italiano Pizza", cuisine:"italian"},
    Restaurant{name:"Sushi House", cuisine:"chinese"},
    Restaurant{name:"Brezel Hut", cuisine:"german"}
];
let iter = restaurants.iter();

// 4\. Create the `LookupTable`
let lookup_table = ironstorm_lookup::LookupTable::from_iter(iter);

// 5\. Find restaurants containing `i`
let mut result_iter = lookup_table.find("i");

// two times 'Italiano pizza', because it's in the lowest bucket
// two times because it has two lower case `i` in the name
assert_eq!(result_iter.next().unwrap().name, "Italiano Pizza");
assert_eq!(result_iter.next().unwrap().name, "Italiano Pizza");

// 'Sushi House', because it's in the second lowest bucket
assert_eq!(result_iter.next().unwrap().name, "Sushi House");

// 'Ami Guy' or ' India Man'
// They are in the same bucket and there is no order within the same bucket
let indian_or_american_1 = result_iter.next().unwrap().name;
assert!(indian_or_american_1=="India Man" || indian_or_american_1=="Ami Guy");

// The other one of 'Ami Guy' or ' India Man'
let indian_or_american_2 = result_iter.next().unwrap().name;
assert!(indian_or_american_2=="India Man" || indian_or_american_2=="Ami Guy");
assert!(indian_or_american_1 != indian_or_american_2);

// No more matches
// "Brezel Hut" doesn't contain an "i" and was not part of the result.
assert!(result_iter.next().is_none());
```

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
