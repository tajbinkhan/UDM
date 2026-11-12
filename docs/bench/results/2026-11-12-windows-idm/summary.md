# Benchmark summary

- Started: 2026-11-12T04:26:50Z on Windows-11-10.0.26300-SP0
- CPU: AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical; RAM: 31.1 GiB
- File: 64 MiB, 5 runs per tool and scenario
- Tools: udm = udm-cli 0.4.0 (repository at b4684a6); idm = 6, 43, 18, 3; curl = curl 8.22.0 (Windows) libcurl/8.22.0 Schannel zlib/1.3.2 WinIDN WinLDAP; wget = GNU Wget 1.21.4 built on mingw32.
- Note: IDM 6.43 build 18, defaults (8 connections) except 'Show download complete dialog' turned off; IDM's main window open; Defender real-time protection on; Samsung 990 PRO NVMe SSD holds the system and the download folder; desktop, Ultimate Performance power plan; aria2c not installed; UDM release build of the claude/rename-udm working tree with the idle-exit and connect-timeout fixes

Median of the runs that got the whole, correct file. CPU is user plus system time. Connections is the most the server saw at once; sent is the bytes it sent over the file's size.

| Scenario | Tool | OK | Median s | Min s | Max s | MiB/s | CPU s | Connections | Sent |
|---|---|---|---|---|---|---|---|---|---|
| clean | idm | 5/5 | 0.25 | 0.25 | 0.31 | 255.0 | 0.22 | 4 | 1.02x |
| clean | curl | 5/5 | 0.06 | 0.06 | 0.11 | 1103.4 | 0.05 | 1 | 1.00x |
| clean | wget | 5/5 | 0.06 | 0.04 | 0.12 | 1084.7 | 0.05 | 1 | 1.00x |
| clean | udm | 5/5 | 0.07 | 0.06 | 0.08 | 941.2 | 0.12 | 8 | 1.10x |
| per-connection-limit | idm | 5/5 | 2.83 | 2.78 | 3.00 | 22.6 | 0.62 | 8 | 1.00x |
| per-connection-limit | curl | 5/5 | 16.00 | 16.00 | 16.01 | 4.0 | 0.00 | 1 | 1.00x |
| per-connection-limit | wget | 5/5 | 16.00 | 16.00 | 16.00 | 4.0 | 0.02 | 1 | 1.00x |
| per-connection-limit | udm | 5/5 | 2.01 | 2.01 | 2.38 | 31.8 | 0.00 | 8 | 1.00x |
| slow-link | idm | 5/5 | 4.19 | 4.19 | 4.20 | 15.3 | 0.25 | 8 | 1.00x |
| slow-link | curl | 5/5 | 4.24 | 4.23 | 4.24 | 15.1 | 0.11 | 1 | 1.00x |
| slow-link | wget | 5/5 | 4.24 | 4.23 | 4.24 | 15.1 | 0.08 | 1 | 1.00x |
| slow-link | udm | 5/5 | 4.02 | 4.02 | 4.02 | 15.9 | 0.02 | 8 | 1.00x |
| drops | idm | 5/5 | 2.02 | 1.87 | 2.93 | 31.6 | 0.38 | 8 | 1.00x |
| drops | curl | 0/5 | failed | | | | | | |
| drops | wget | 5/5 | 113.16 | 113.13 | 113.22 | 0.6 | 0.02 | 1 | 1.00x |
| drops | udm | 5/5 | 1.78 | 1.42 | 2.00 | 35.9 | 0.02 | 8 | 1.00x |
| two-per-client | idm | 5/5 | 8.21 | 8.21 | 8.22 | 7.8 | 0.39 | 2 | 1.00x |
| two-per-client | curl | 5/5 | 16.00 | 16.00 | 16.01 | 4.0 | 0.00 | 1 | 1.00x |
| two-per-client | wget | 5/5 | 16.00 | 16.00 | 16.00 | 4.0 | 0.03 | 1 | 1.00x |
| two-per-client | udm | 5/5 | 8.01 | 8.01 | 8.37 | 8.0 | 0.02 | 2 | 1.00x |
| slow-start | idm | 5/5 | 6.34 | 6.24 | 6.50 | 10.1 | 0.17 | 14 | 1.00x |
| slow-start | curl | 5/5 | 17.00 | 17.00 | 17.00 | 3.8 | 0.00 | 1 | 1.00x |
| slow-start | wget | 5/5 | 17.00 | 17.00 | 17.00 | 3.8 | 0.02 | 1 | 1.00x |
| slow-start | udm | 5/5 | 4.01 | 4.01 | 4.37 | 16.0 | 0.02 | 8 | 1.00x |
| wrong-ranges | idm | 5/5 | 16.14 | 16.13 | 16.15 | 4.0 | 0.70 | 5 | 1.01x |
| wrong-ranges | curl | 5/5 | 16.00 | 16.00 | 16.00 | 4.0 | 0.00 | 1 | 1.00x |
| wrong-ranges | wget | 5/5 | 16.00 | 16.00 | 16.00 | 4.0 | 0.00 | 1 | 1.00x |
| wrong-ranges | udm | 5/5 | 3.26 | 3.26 | 3.62 | 19.6 | 0.02 | 8 | 1.01x |
| no-ranges | idm | 5/5 | 4.14 | 4.13 | 4.15 | 15.5 | 0.16 | 3 | 1.00x |
| no-ranges | curl | 5/5 | 4.01 | 4.01 | 4.01 | 16.0 | 0.02 | 1 | 1.00x |
| no-ranges | wget | 5/5 | 4.01 | 4.01 | 4.01 | 16.0 | 0.03 | 1 | 1.00x |
| no-ranges | udm | 5/5 | 4.02 | 4.02 | 4.02 | 15.9 | 0.02 | 2 | 1.00x |
