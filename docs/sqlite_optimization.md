# SQLite Performance Optimization Experience

## Background

In the `restic-123pan` project, we use SQLite as a persistent metadata cache to speed up file listings and path resolutions. As the number of files grew (e.g., ~600,000 files), specific queries began to trigger slow query warnings, exceeding the 1-second threshold.

A specific problematic query involved fetching all files from multiple subdirectories simultaneously:

```sql
WARN sqlx::query: slow statement: execution time exceeded alert threshold
summary="SELECT ... FROM file_nodes WHERE parent_id IN (?, ?, ...) AND is_dir = ?"
rows_returned=577262 elapsed=17.70s
```

## Anti-Pattern: Logic Refactoring (The Wrong Approach)

Our initial reaction was to refactor the application logic to "break down" the query. We attempted to query each subdirectory individually in a loop (`for subdir in subdirs { query... }`) instead of using a single `parent_id IN (...)` clause.

**Why this was rejected:**
1.  **Inefficiency**: It introduces significant overhead from multiple database round-trips.
2.  **Complexity**: It makes the code more verbose and strictly coupled to the current implementation detail of subdirectory structures.
3.  **"Band-aid" solution**: It treats the symptom (slow query) rather than the root cause (database configuration).

## Preferred Solution: Database Tuning

The correct approach is to optimize the SQLite engine configuration itself. SQLite's default settings are very conservative and often not suitable for high-performance or read-heavy caching scenarios.

### Applied Optimizations

We applied the following PRAGMA statements immediately after connecting to the database:

```rust
// Enable SQLite performance optimizations
db.execute(sea_orm::Statement::from_string(
    sea_orm::DatabaseBackend::Sqlite,
    "PRAGMA journal_mode=WAL;
     PRAGMA synchronous=NORMAL;
     PRAGMA cache_size=-64000; -- ~64MB
     PRAGMA temp_store=MEMORY;
     PRAGMA mmap_size=30000000000;",
))
.await?;
```

### Detailed Explanation

1.  **`PRAGMA journal_mode=WAL;`**
    *   **Write-Ahead Logging**. This is the single most impactful change.
    *   It allows concurrent readers and writers (blockers are removed).
    *   Significantly improves write performance and overall concurrency.

2.  **`PRAGMA synchronous=NORMAL;`**
    *   In WAL mode, `NORMAL` is safe for most applications (data is only lost if the OS crashes/loses power, not if the app crashes).
    *   It reduces the number of `fsync()` calls, drastically improving write speed compared to the default `FULL`.

3.  **`PRAGMA cache_size=-64000;`**
    *   Sets the page cache size.
    *   A negative number acts as a hint in **kilobytes**. `-64000` sets the cache to ~64MB.
    *   This ensures that working sets (like our file list) are more likely to stay in memory, reducing disk reads.

4.  **`PRAGMA temp_store=MEMORY;`**
    *   Forces temporary tables and indices to be stored in RAM instead of on disk.
    *   Useful for complex queries that require temporary structures (sorting, grouping, or large `IN` clauses).

5.  **`PRAGMA mmap_size=30000000000;`**
    *   **Memory-Mapped I/O**. Sets the limit for memory-mapped I/O to ~30GB.
    *   Allows SQLite to access the database file directly via memory addresses, bypassing the OS filesystem cache layers.
    *   For read-heavy workloads (like checking if a file exists or listing directories), this can provide near-memory speeds.

## Conclusion

Before refactoring code to work around database performance limitations, always ensure the database itself is tuned for the workload. For SQLite, `WAL` mode and `mmap` are critical for performance in read-heavy or concurrent applications.
