EP-003 live-fire reproduction

The transcript in ../livefire.log was produced by:

  1. ./target/debug/vector-tools.exe db setup   --db-path <dir>/final.db
  2. python3 drive.py <dir>/final.db seed       # independent sqlite3 writes
  3. ./target/debug/vector-tools.exe db backup  --db-path <dir>/final.db --dest <dir>/final.backup.sqlite
  4. python3 drive.py <dir>/final.db fk         # proves FK enforcement
  5. python3 drive.py <dir>/final.db delete     # destroy live data
  6. ./target/debug/vector-tools.exe db restore --db-path <dir>/final.db --source <dir>/final.backup.sqlite
  7. python3 drive.py <dir>/final.db read       # independent readback

Generated .db and .sqlite files are deliberately not committed: they are
reproducible from the steps above and would be binary noise in the repository.
