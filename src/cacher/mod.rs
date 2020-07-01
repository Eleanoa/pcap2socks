use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::io;
use std::ops::Bound::Included;

/// Represents the initial size of cache.
const INITIAL_SIZE: usize = 64 * 1024;
/// Represents the expansion factor of the cache. The cache will be expanded by the factor.
const EXPANSION_FACTOR: f64 = 1.5;

/// Represents the max distance of u32 values between packets in an u32 window.
const MAX_U32_WINDOW_SIZE: usize = 4 * 1024 * 1024;

/// Represents the linear cache.
#[derive(Debug)]
pub struct Cacher {
    buffer: Vec<u8>,
    unbounded: bool,
    sequence: u32,
    head: usize,
    size: usize,
}

impl Cacher {
    /// Creates a new `Cacher`.
    pub fn new(sequence: u32) -> Cacher {
        Cacher {
            buffer: vec![0; INITIAL_SIZE],
            unbounded: false,
            sequence,
            head: 0,
            size: 0,
        }
    }

    /// Creates a new `Cacher` which can increase its size dynamically.
    pub fn new_unbounded(sequence: u32) -> Cacher {
        let mut cacher = Cacher::new(sequence);
        cacher.unbounded = true;

        cacher
    }

    /// Appends some bytes to the end of the cache.
    pub fn append(&mut self, buffer: &[u8]) -> io::Result<()> {
        if buffer.len() > self.buffer.len() - self.size {
            if self.is_unbounded() {
                // Extend the buffer
                let size = max(
                    (self.buffer.len() as f64 * EXPANSION_FACTOR) as usize,
                    self.buffer.len() + buffer.len(),
                );

                let mut new_buffer = vec![0u8; size];

                // From the head to the end of the buffer
                let length_a = min(self.size, self.buffer.len() - self.head);
                new_buffer[..length_a]
                    .copy_from_slice(&self.buffer[self.head..self.head + length_a]);

                // From the begin of the buffer to the tail
                let length_b = self.size - length_a;
                if length_b > 0 {
                    new_buffer[length_a..length_a + length_b]
                        .copy_from_slice(&self.buffer[..length_b]);
                }

                self.buffer = new_buffer;
                self.head = 0;
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "cache is full"));
            }
        }

        // From the tail to the end of the buffer
        let mut length_a = 0;
        if self.head + self.size < self.buffer.len() {
            length_a = min(buffer.len(), self.buffer.len() - (self.head + self.size));
            self.buffer[self.head + self.size..self.head + self.size + length_a]
                .copy_from_slice(&buffer[..length_a]);
        }

        // From the begin of the buffer to the head
        let length_b = buffer.len() - length_a;
        if length_b > 0 {
            self.buffer[..length_b].copy_from_slice(&buffer[length_a..]);
        }

        self.size += buffer.len();

        Ok(())
    }

    // Invalidates cache to the certain sequence.
    pub fn invalidate_to(&mut self, sequence: u32) {
        let size = sequence
            .checked_sub(self.sequence)
            .unwrap_or_else(|| u32::MAX - self.sequence + sequence) as usize;

        if size <= MAX_U32_WINDOW_SIZE as usize {
            self.sequence = sequence;
            self.size = self.size.checked_sub(size).unwrap_or(0);
            if self.size == 0 {
                self.head = 0;
            } else {
                self.head = (self.head + (size % self.buffer.len())) % self.buffer.len();
            }
        }
    }

    /// Get the buffer from the certain sequence of the cache in the given size.
    pub fn get(&self, sequence: u32, size: usize) -> io::Result<Vec<u8>> {
        if size == 0 {
            return Ok(Vec::new());
        }
        let distance = sequence
            .checked_sub(self.sequence)
            .unwrap_or_else(|| sequence + (u32::MAX - self.sequence))
            as usize;
        if distance > self.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cache at the certain sequence does not exist",
            ));
        }
        if self.size - distance < size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "request size too big",
            ));
        }

        let mut vector = vec![0u8; size];

        // From the head to the end of the buffer
        let head = self.head + distance;
        let head = head.checked_sub(self.buffer.len()).unwrap_or(head);
        let length_a = min(size, self.buffer.len() - head);
        vector[..length_a].copy_from_slice(&self.buffer[head..head + length_a]);

        // From the begin of the buffer to the tail
        let length_b = size - length_a;
        if length_b > 0 {
            vector[length_a..].copy_from_slice(&self.buffer[..length_b]);
        }

        Ok(vector)
    }

    /// Get all the buffer of the cache.
    pub fn get_all(&self) -> io::Result<Vec<u8>> {
        self.get(self.sequence, self.size)
    }

    /// Get the sequence of the cache.
    pub fn get_sequence(&self) -> u32 {
        self.sequence
    }

    /// Get the size of the cache.
    pub fn get_size(&self) -> usize {
        self.size
    }

    fn is_unbounded(&self) -> bool {
        self.unbounded
    }

    /// Returns if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

/// Represents the random cache.
#[derive(Debug)]
pub struct RandomCacher {
    buffer: Vec<u8>,
    unbounded: bool,
    sequence: u32,
    head: usize,
    /// Represents the expected size from the head to the tail. NOT all the bytes in [head, head + size) are filled.
    size: usize,
    /// Represents edges of filled values. Use an u64 instead of an u32 because the sequence is used as a ring.
    edges: BTreeMap<u64, usize>,
}

impl RandomCacher {
    /// Creates a new `RandomCacher`.
    pub fn new(sequence: u32) -> RandomCacher {
        RandomCacher {
            buffer: vec![0u8; INITIAL_SIZE],
            unbounded: false,
            sequence,
            head: 0,
            size: 0,
            edges: BTreeMap::new(),
        }
    }

    /// Creates a new `RandomCacher` which can increase its size dynamically.
    pub fn new_unbounded(sequence: u32) -> RandomCacher {
        let mut cacher = RandomCacher::new(sequence);
        cacher.unbounded = true;

        cacher
    }

    /// Appends some bytes to the cache and returns continuous bytes from the beginning.
    pub fn append(&mut self, sequence: u32, buffer: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let sub_sequence = sequence
            .checked_sub(self.sequence)
            .unwrap_or_else(|| sequence + (u32::MAX - self.sequence))
            as usize;
        if sub_sequence > MAX_U32_WINDOW_SIZE {
            return Ok(None);
        }

        let size = sub_sequence + buffer.len();
        if size > self.buffer.len() {
            if self.is_unbounded() {
                // Extend the buffer
                let size = max((self.buffer.len() as f64 * EXPANSION_FACTOR) as usize, size);

                let mut new_buffer = vec![0u8; size];

                let filled = self.get_filled();
                for (sequence, recv_next) in filled {
                    // Place in the new buffer
                    let new_head = sequence
                        .checked_sub(self.sequence)
                        .unwrap_or_else(|| sequence + (u32::MAX - self.sequence))
                        as usize;
                    let new_head = new_head.checked_sub(self.buffer.len()).unwrap_or(new_head);
                    let new_tail = recv_next
                        .checked_sub(self.sequence)
                        .unwrap_or_else(|| recv_next + (u32::MAX - self.sequence))
                        as usize;
                    let new_tail = new_tail.checked_sub(self.buffer.len()).unwrap_or(new_tail);

                    // Place in the original buffer
                    let head = self.head + new_head;
                    let head = head.checked_sub(self.buffer.len()).unwrap_or(head);
                    let tail = self.head + new_tail;
                    let tail = tail.checked_sub(self.buffer.len()).unwrap_or(tail);

                    if tail < head {
                        // From the head to the end of the buffer
                        let length_a = self.buffer.len() - head;
                        new_buffer[new_head..new_head + length_a]
                            .copy_from_slice(&self.buffer[head..]);

                        // From the begin of the buffer to the tail
                        new_buffer[new_head + length_a..new_tail]
                            .copy_from_slice(&self.buffer[..tail]);
                    } else {
                        new_buffer[new_head..new_tail].copy_from_slice(&self.buffer[head..tail]);
                    }
                }

                self.buffer = new_buffer;
                self.head = 0;
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "cache is full"));
            }
        }

        // TODO: the procedure may by optimized to copy valid bytes only
        // To the end of the buffer
        let mut length_a = 0;
        if self.buffer.len() - self.head > sub_sequence {
            length_a = min(self.buffer.len() - self.head - sub_sequence, buffer.len());
            self.buffer[self.head + sub_sequence..self.head + sub_sequence + length_a]
                .copy_from_slice(&buffer[..length_a]);
        }

        // From the begin of the buffer
        let length_b = buffer.len() - length_a;
        if length_b > 0 {
            self.buffer[..length_b].copy_from_slice(&buffer[length_a..]);
        }

        // Update size
        let recv_next = sequence
            .checked_add(buffer.len() as u32)
            .unwrap_or_else(|| buffer.len() as u32 - (u32::MAX - sequence));
        let record_recv_next = self
            .sequence
            .checked_add(self.size as u32)
            .unwrap_or_else(|| self.size as u32 - (u32::MAX - self.sequence));
        let sub_recv_next = recv_next
            .checked_sub(record_recv_next)
            .unwrap_or_else(|| recv_next + (u32::MAX - record_recv_next));
        if sub_recv_next as usize <= MAX_U32_WINDOW_SIZE {
            self.size += sub_recv_next as usize;
        }

        // Insert and merge ranges
        {
            let mut sequence = sequence as u64;
            if (sequence as u32) < self.sequence {
                sequence += u32::MAX as u64;
            }

            // Select ranges which can be merged in a loop
            let mut end = sequence + buffer.len() as u64;
            loop {
                let mut pop_keys = Vec::new();
                for (&key, &value) in self.edges.range((
                    Included(&sequence),
                    Included(&(sequence + buffer.len() as u64)),
                )) {
                    pop_keys.push(key);
                    end = max(end, key + value as u64);
                }

                if pop_keys.len() <= 0 {
                    break;
                }

                // Pop
                for ref pop_key in pop_keys {
                    self.edges.remove(pop_key);
                }
            }

            // Select the previous range if exists
            let mut prev_key = None;
            for &key in self.edges.keys() {
                if key < sequence {
                    prev_key = Some(key);
                }
            }

            // Merge previous range
            let mut size = end - sequence;
            if let Some(prev_key) = prev_key {
                let prev_size = *self.edges.get(&prev_key).unwrap();
                if prev_key + (prev_size as u64) >= sequence {
                    size += sequence - prev_key;
                    sequence = prev_key;
                }
            }

            // Insert range
            self.edges.insert(sequence, size as usize);
        }

        // Pop if possible
        let first_key = *self.edges.keys().next().unwrap();
        if first_key as u32 == self.sequence {
            let size = self.edges.remove(&first_key).unwrap();

            // Shrink range sequence is possible
            if ((u32::MAX - self.sequence) as usize) < size {
                let keys: Vec<_> = self.edges.keys().map(|x| *x).collect();

                for key in keys {
                    let value = self.edges.remove(&key).unwrap();
                    self.edges.insert(key - u32::MAX as u64, value);
                }
            }

            let mut vector = vec![0u8; size];

            // From the head to the end of the buffer
            let length_a = min(size, self.buffer.len() - self.head);
            vector[..length_a].copy_from_slice(&self.buffer[self.head..self.head + length_a]);

            // From the begin of the buffer to the tail
            let length_b = size - length_a;
            if length_b > 0 {
                vector[length_a..].copy_from_slice(&self.buffer[..length_b]);
            }

            self.sequence = self
                .sequence
                .checked_add(size as u32)
                .unwrap_or_else(|| size as u32 - (u32::MAX - self.sequence));
            self.head = (self.head + (size % self.buffer.len())) % self.buffer.len();
            self.size -= vector.len();

            return Ok(Some(vector));
        }

        Ok(None)
    }

    /// Get the sequence of the cache.
    pub fn get_sequence(&self) -> u32 {
        self.sequence
    }

    /// Get the receive next of the cache.
    pub fn get_recv_next(&self) -> u32 {
        self.sequence
            .checked_add(self.size as u32)
            .unwrap_or_else(|| self.size as u32 - (u32::MAX - self.sequence))
    }

    /// Get the remaining size of the `RandomCacher`.
    pub fn get_remaining_size(&self) -> u16 {
        if self.buffer.len() - self.size > u16::MAX as usize {
            u16::MAX
        } else {
            (self.buffer.len() - self.size) as u16
        }
    }

    /// Get the filled edges of the `RandomCacher`.
    pub fn get_filled(&self) -> Vec<(u32, u32)> {
        let mut v = Vec::new();
        for (&sequence, &size) in &self.edges {
            let begin = sequence.checked_sub(u32::MAX as u64).unwrap_or(sequence) as u32;
            let end = begin
                .checked_add(size as u32)
                .unwrap_or_else(|| size as u32 - (u32::MAX - begin));
            v.push((begin, end));
        }

        v
    }

    fn is_unbounded(&self) -> bool {
        self.unbounded
    }

    /// Returns if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}
