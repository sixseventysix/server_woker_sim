## how to run tests
### run all tests:
```bash
cargo test -- --test-threads=1 --nocapture
```

### run an individual test:
```bash
cargo test <test_name> --nocapture
```

> Note: For better readability, pipe the cargo test command to a file, the logs can get large.