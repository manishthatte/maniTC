// stdlib/std/collections.mt
// Generic collection types for maniT.
//
// This module provides heap-allocated, growable data structures.  All types
// are generic over their element/key/value types.  The TernaryTrie is unique
// to maniT: it is indexed by trit sequences, making it a natural sparse map
// over balanced ternary key spaces.
//
// Usage:
//   use std::collections;
//   let v: Vec<int> = Vec::new();

// ---------------------------------------------------------------------------
// Vec<T> — dynamic array
// ---------------------------------------------------------------------------

// A contiguous, heap-allocated, growable array of elements of type T.
struct Vec<T> {
    // native backing buffer — do not access fields directly
}

impl<T> Vec<T> {
    // Create an empty Vec.
    fn new() -> Vec<T> ;  // native

    // Create a Vec with the given initial capacity (avoids early reallocations).
    fn with_capacity(cap: int) -> Vec<T> ;  // native

    // Create a Vec from a fixed-size array literal.
    fn from(arr: [T]) -> Vec<T> ;  // native

    // Append an element to the end.  Amortised O(1).
    fn push(self, val: T) ;  // native

    // Remove and return the last element.  Returns Unknown if empty.
    fn pop(self) -> T ;  // native

    // Return the element at index `i`.  Panics on out-of-bounds.
    fn get(self, i: int) -> T ;  // native

    // Set the element at index `i`.  Panics on out-of-bounds.
    fn set(self, i: int, val: T) ;  // native

    // Number of elements currently stored.
    fn len(self) -> int ;  // native

    // Return true if the Vec contains no elements.
    fn is_empty(self) -> bool ;  // native

    // Allocated capacity (number of elements before the next reallocation).
    fn capacity(self) -> int ;  // native

    // Remove the element at index `i`, shifting subsequent elements left.  O(n).
    fn remove(self, i: int) -> T ;  // native

    // Insert `val` at index `i`, shifting subsequent elements right.  O(n).
    fn insert(self, i: int, val: T) ;  // native

    // Remove all elements without releasing the allocated memory.
    fn clear(self) ;  // native

    // Return true if the Vec contains `val` (uses == for comparison).
    fn contains(self, val: T) -> bool ;  // native

    // Return the index of the first occurrence of `val`, or -1 if absent.
    fn index_of(self, val: T) -> int ;  // native

    // Reverse the order of elements in place.
    fn reverse(self) ;  // native

    // Sort elements in ascending order (requires T: Ord).
    fn sort(self) ;  // native

    // Return a new Vec containing elements in the range [start, end).
    fn slice(self, start: int, end: int) -> Vec<T> ;  // native

    // Append all elements from `other` to the end of this Vec.
    fn extend(self, other: Vec<T>) ;  // native

    // Apply `f` to every element, returning a new Vec of results.
    fn map<U>(self, f: fn(T) -> U) -> Vec<U> ;  // native

    // Return a new Vec containing only elements for which `pred` returns true.
    fn filter(self, pred: fn(T) -> bool) -> Vec<T> ;  // native

    // Reduce the Vec to a single value using `f`, starting from `init`.
    fn fold<U>(self, init: U, f: fn(U, T) -> U) -> U ;  // native

    // Call `f` for every element (in order) for its side effects.
    fn for_each(self, f: fn(T)) ;  // native
}

// ---------------------------------------------------------------------------
// Map<K, V> — hash map
// ---------------------------------------------------------------------------

// An unordered associative container mapping keys of type K to values of V.
// Keys must implement Hash and Eq.
struct Map<K, V> {
    // native hash table — do not access fields directly
}

impl<K, V> Map<K, V> {
    // Create an empty Map.
    fn new() -> Map<K, V> ;  // native

    // Create a Map with the given initial capacity.
    fn with_capacity(cap: int) -> Map<K, V> ;  // native

    // Insert or overwrite the value for `key`.  Returns the previous value
    // if the key was already present, or Unknown otherwise.
    fn insert(self, key: K, val: V) -> V ;  // native

    // Return the value for `key`.  Panics if the key is absent.
    fn get(self, key: K) -> V ;  // native

    // Return the value for `key`, or `default` if absent.
    fn get_or(self, key: K, default: V) -> V ;  // native

    // Remove the entry for `key`.  Returns the removed value, or panics.
    fn remove(self, key: K) -> V ;  // native

    // Return true if the Map contains `key`.
    fn contains_key(self, key: K) -> bool ;  // native

    // Number of key-value pairs stored.
    fn len(self) -> int ;  // native

    // Return true if the Map is empty.
    fn is_empty(self) -> bool ;  // native

    // Remove all entries.
    fn clear(self) ;  // native

    // Return a Vec of all keys (order is unspecified).
    fn keys(self) -> Vec<K> ;  // native

    // Return a Vec of all values (order matches keys()).
    fn values(self) -> Vec<V> ;  // native

    // Iterate over (key, value) pairs, calling `f` for each.
    fn for_each(self, f: fn(K, V)) ;  // native
}

// ---------------------------------------------------------------------------
// Set<T> — hash set
// ---------------------------------------------------------------------------

// An unordered collection of unique elements of type T.
struct Set<T> {
    // native hash set — do not access fields directly
}

impl<T> Set<T> {
    // Create an empty Set.
    fn new() -> Set<T> ;  // native

    // Insert `val`.  Returns true if the value was not already present.
    fn insert(self, val: T) -> bool ;  // native

    // Remove `val`.  Returns true if the value was present.
    fn remove(self, val: T) -> bool ;  // native

    // Return true if the Set contains `val`.
    fn contains(self, val: T) -> bool ;  // native

    // Number of elements.
    fn len(self) -> int ;  // native

    // Return true if the Set is empty.
    fn is_empty(self) -> bool ;  // native

    // Remove all elements.
    fn clear(self) ;  // native

    // Return a new Set containing elements present in both sets (intersection).
    fn intersection(self, other: Set<T>) -> Set<T> ;  // native

    // Return a new Set containing all elements from either set (union).
    fn union(self, other: Set<T>) -> Set<T> ;  // native

    // Return a new Set containing elements in `self` but not in `other`.
    fn difference(self, other: Set<T>) -> Set<T> ;  // native

    // Return true if every element of `self` is also in `other`.
    fn is_subset(self, other: Set<T>) -> bool ;  // native

    // Call `f` for every element.
    fn for_each(self, f: fn(T)) ;  // native
}

// ---------------------------------------------------------------------------
// Deque<T> — double-ended queue
// ---------------------------------------------------------------------------

// A growable ring-buffer deque supporting O(1) push/pop at both ends.
struct Deque<T> {
    // native ring buffer — do not access fields directly
}

impl<T> Deque<T> {
    // Create an empty Deque.
    fn new() -> Deque<T> ;  // native

    // Push `val` onto the front.
    fn push_front(self, val: T) ;  // native

    // Push `val` onto the back.
    fn push_back(self, val: T) ;  // native

    // Remove and return the front element.  Panics if empty.
    fn pop_front(self) -> T ;  // native

    // Remove and return the back element.  Panics if empty.
    fn pop_back(self) -> T ;  // native

    // Peek at the front element without removing it.  Panics if empty.
    fn front(self) -> T ;  // native

    // Peek at the back element without removing it.  Panics if empty.
    fn back(self) -> T ;  // native

    // Number of elements.
    fn len(self) -> int ;  // native

    // Return true if the Deque is empty.
    fn is_empty(self) -> bool ;  // native

    // Remove all elements.
    fn clear(self) ;  // native

    // Access element at index `i` (0 = front).  Panics on out-of-bounds.
    fn get(self, i: int) -> T ;  // native
}

// ---------------------------------------------------------------------------
// TernaryTrie<V> — trie indexed by trit sequences (maniT-native)
// ---------------------------------------------------------------------------

// A prefix tree (trie) whose edges are labeled by trit values (+, 0, -).
// Each node can have at most three children, one per trit.
//
// This structure is uniquely suited to maniT: any balanced ternary number
// or ternary key can be decomposed into trits and used as a trie path,
// giving O(d) lookup/insert where d is the number of trits in the key.
//
// Typical use cases:
//   - Routing tables keyed by ternary addresses
//   - Memoisation of functions over t27 domains
//   - Sparse representation of ternary state spaces
//   - Prefix compression of ternary data streams
struct TernaryTrie<V> {
    // native trie node — do not access fields directly
}

impl<V> TernaryTrie<V> {
    // Create an empty trie.
    fn new() -> TernaryTrie<V> ;  // native

    // Insert a value at the path described by `key` (a trit slice, MST-first).
    // Overwrites any existing value at the same path.
    fn insert(self, key: [trit], val: V) ;  // native

    // Look up the value at `key`.  Returns the value or panics if absent.
    fn get(self, key: [trit]) -> V ;  // native

    // Return true if the trie contains an entry at `key`.
    fn contains(self, key: [trit]) -> bool ;  // native

    // Remove the entry at `key`.  Returns the removed value or panics.
    fn remove(self, key: [trit]) -> V ;  // native

    // Return all keys that share `prefix` (MST-first trit slice).
    // Useful for autocomplete-style queries over ternary key spaces.
    fn keys_with_prefix(self, prefix: [trit]) -> Vec<[trit]> ;  // native

    // Insert using an int key (automatically converted to balanced ternary).
    fn insert_int(self, key: int, val: V) ;  // native

    // Look up using an int key.
    fn get_int(self, key: int) -> V ;  // native

    // Return true if the trie contains an int key.
    fn contains_int(self, key: int) -> bool ;  // native

    // Total number of stored entries.
    fn len(self) -> int ;  // native

    // Return true if the trie contains no entries.
    fn is_empty(self) -> bool ;  // native

    // Remove all entries, freeing memory.
    fn clear(self) ;  // native

    // Call `f(key, value)` for every entry in depth-first (lexicographic) order.
    fn for_each(self, f: fn([trit], V)) ;  // native

    // Return a Vec of all stored (key, value) pairs in lexicographic order.
    fn entries(self) -> Vec<([trit], V)> ;  // native

    // Merge another trie into this one.  On key collision `f` is called with
    // the existing value and the incoming value; its return value is stored.
    fn merge(self, other: TernaryTrie<V>, f: fn(V, V) -> V) ;  // native

    // Return the number of trie nodes currently allocated (for diagnostics).
    fn node_count(self) -> int ;  // native
}
