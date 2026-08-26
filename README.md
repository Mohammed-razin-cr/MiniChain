# MiniChain

MiniChain is a learning project that shows how a blockchain can be used to verify records such as certificates, achievements, and official documents.

Think of it as a digital notebook shared by several trusted schools or offices. Every new entry is connected to the entry before it. If somebody secretly changes an old entry, the connections no longer match and MiniChain reports that something was altered.

MiniChain is not a cryptocurrency. It has no coins, wallets, mining, or payments.

## A simple example

Imagine that a university issues a course certificate to a student. MiniChain can store a record like this:

```text
Certificate ID: CERT-2026-1042
Student: Razin
Course: Blockchain Fundamentals
Institution: Example University
Issued: 26 August 2026
File fingerprint: a84c19e7f2...
Status: Valid
```

The university signs the record with its private digital key. Later, an employer can check that:

- the certificate record exists;
- it was signed by the expected university;
- its details have not been changed; and
- it has not been revoked.

If even one part of the certificate changes, its digital fingerprint changes too, so the verification fails.

MiniChain currently stores certificate information and fingerprints. It does not yet provide a screen for uploading PDF or image files. File upload and automatic fingerprint generation would be a useful next feature.

## How it works

1. An authorized organization creates and signs a record.
2. The record waits with other new records to be added.
3. Trusted validators check the proposed group of records.
4. Once enough validators approve it, the group becomes a block.
5. The block is connected to the previous block using a digital fingerprint.
6. Other nodes receive and save the same block.
7. Anyone with permission can later verify the record.

Some common blockchain words used in this project:

| Term | Meaning in MiniChain |
| --- | --- |
| Transaction | One requested change, such as creating or revoking a certificate record |
| Block | A group of approved transactions |
| Chain | Blocks connected in order from oldest to newest |
| Hash | A digital fingerprint used to detect changes |
| Validator | A trusted organization allowed to check and approve blocks |
| Node | A computer running MiniChain and keeping a copy of the records |
| Consensus | The rule used by validators to agree on a block |
| Snapshot | A saved recovery point for the local database |

## What is included

- A Rust blockchain and node application
- Digital signatures for records and node messages
- Tamper detection for transactions, blocks, and stored data
- A two-thirds validator approval rule
- Local database storage, recovery, and snapshots
- Communication and synchronization between trusted nodes
- A REST API and live WebSocket events
- A command-line interface for operating and inspecting nodes
- A React dashboard for viewing chain and network activity
- Automated tests for security, networking, recovery, and API behavior

## Current limitations

MiniChain is a working prototype, not a production service. The validator approval rules exist, but live nodes do not yet coordinate the complete approval process or produce blocks automatically. A submitted record can therefore remain pending until a block is created and committed.

MiniChain provides evidence that data was changed; it does not make data impossible to rewrite. Someone who controls every validator key and every database copy could create a different history.

The built-in network and API services do not provide TLS encryption. They should only be used on a trusted local network unless TLS is added in front of them.

## Requirements

- Rust stable with Edition 2024 support
- Node.js 22.13 or newer
- npm
- PowerShell 7 for the included Windows scripts

## Build the project

From the repository folder:

```text
cargo build --release

cd frontend
npm ci
npm run build
cd ..
```

## Run the demonstration

This script starts three temporary nodes, checks that they can communicate, creates sample records, tests recovery and tamper detection, and removes the temporary data when it finishes:

```text
powershell -ExecutionPolicy Bypass -File scripts/demo.ps1 -SkipBuild
```

Remove `-SkipBuild` if the release application has not been built. Add `-KeepData` to keep the temporary databases and logs.

A smaller demonstration can be run with:

```text
cargo run --release -- demo run
```

## Run three local nodes

The files in `config/` describe a three-node local network. First, generate a separate identity for each node:

```text
cargo run --release -- identity init --node-id node-01 --output data/node-01.key
cargo run --release -- identity init --node-id node-02 --output data/node-02.key
cargo run --release -- identity init --node-id node-03 --output data/node-03.key
```

Each command prints a public key. Copy those keys into the matching `trusted_peers` entries in `config/node-01.toml`, `config/node-02.toml`, and `config/node-03.toml`.

Start each node in a separate terminal:

```text
cargo run --release -- --config config/node-01.toml node start
cargo run --release -- --config config/node-02.toml node start
cargo run --release -- --config config/node-03.toml node start
```

The example API tokens are only for local development:

```text
Viewer:   minichain-dev-viewer-change-me
Operator: minichain-dev-operator-change-me
Admin:    minichain-dev-admin-change-me
```

Replace them before using MiniChain on a shared network.

## Open the dashboard

After starting a node:

```text
cd frontend
npm run dev
```

Open the address shown in the terminal. Connect to `http://127.0.0.1:9201/api/v1` and enter one of the local API tokens above.

## Useful commands

```text
# Check whether a node is running
cargo run --release -- node status

# Check the blockchain for changes or broken links
cargo run --release -- chain validate

# List saved blocks
cargo run --release -- block list

# View connected nodes
cargo run --release -- network peers

# Run a general health check
cargo run --release -- diagnostics
```

Commands that connect to the API read the token from `MINICHAIN_API_TOKEN`. Use `cargo run -- --help` to see every available command.

## Project folders

```text
src/        Main Rust application code
tests/      Automated tests
config/     Example settings for three local nodes
frontend/   Web dashboard
scripts/    Demo and release scripts
.github/    Automatic checks run by GitHub
```

## Run the tests

Backend:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Dashboard:

```text
cd frontend
npm ci
npm test
npm run build
npm run lint
```

## License

MiniChain is released under the [MIT License](LICENSE).
