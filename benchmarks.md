source: [https://github.com/karlivory/rust-web-benchmarks](https://github.com/karlivory/rust-web-benchmarks)

## Benchmarks

**Specs**: Intel(R) Core(TM) Ultra 5 125U, 14 cpus.

`rewrk --threads 14 --connections 64 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|0.25ms|0.15ms|0.01ms|4.06ms|260360.14|32.28MB/Sec|4.6MB|
|axum|0.24ms|0.11ms|0.01ms|2.39ms|265213.63|32.88MB/Sec|5.8MB|
|khttp|0.22ms|0.09ms|0.02ms|3.72ms|285131.81|38.34MB/Sec|1.7MB|
|may_minihttp|0.22ms|0.15ms|0.01ms|4.45ms|291608.55|39.21MB/Sec|13.4MB|
|poem|0.25ms|0.11ms|0.01ms|2.10ms|256515.74|31.80MB/Sec|8.1MB|

`rewrk --threads 14 --connections 256 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|0.85ms|0.51ms|0.02ms|8.53ms|300913.76|37.31MB/Sec|7.7MB|
|axum|0.93ms|0.46ms|0.02ms|4.80ms|273773.19|33.94MB/Sec|10.3MB|
|khttp|0.81ms|0.38ms|0.02ms|6.65ms|317228.41|42.66MB/Sec|1.8MB|
|may_minihttp|0.80ms|0.48ms|0.02ms|9.16ms|319537.66|42.97MB/Sec|26.2MB|
|poem|0.99ms|0.49ms|0.02ms|5.50ms|258581.73|32.06MB/Sec|13.8MB|

`rewrk --threads 14 --connections 512 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|1.68ms|1.02ms|0.02ms|25.32ms|305154.48|37.83MB/Sec|11.7MB|
|axum|1.84ms|0.91ms|0.02ms|12.38ms|277492.38|34.40MB/Sec|16.6MB|
|khttp|1.57ms|0.75ms|0.02ms|24.07ms|326118.37|43.85MB/Sec|1.8MB|
|may_minihttp|1.59ms|0.92ms|0.02ms|18.36ms|321230.05|43.20MB/Sec|43.4MB|
|poem|1.96ms|0.97ms|0.03ms|13.27ms|260965.63|32.35MB/Sec|22.0MB|

