Uses `serve_epoll`.

Source: [https://github.com/karlivory/rust-web-benchmarks](https://github.com/karlivory/rust-web-benchmarks)

## Benchmarks

**Specs**: Intel(R) Core(TM) Ultra 5 125U, 14 cpus.

`rewrk --threads 14 --connections 64 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|0.25ms|0.15ms|0.01ms|3.36ms|257711.79|31.95MB/Sec|4.6MB|
|axum|0.24ms|0.11ms|0.01ms|3.13ms|265392.10|32.90MB/Sec|5.6MB|
|khttp|0.22ms|0.09ms|0.01ms|3.37ms|287027.13|38.60MB/Sec|1.7MB|
|may_minihttp|0.22ms|0.15ms|0.01ms|4.40ms|294760.33|39.64MB/Sec|13.5MB|
|poem|0.25ms|0.11ms|0.01ms|2.72ms|253944.78|31.48MB/Sec|8.0MB|

`rewrk --threads 14 --connections 256 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|0.84ms|0.50ms|0.02ms|7.81ms|302923.99|37.56MB/Sec|7.4MB|
|axum|0.92ms|0.45ms|0.02ms|6.82ms|277900.13|34.45MB/Sec|10.4MB|
|khttp|0.79ms|0.38ms|0.02ms|6.20ms|325103.03|43.72MB/Sec|1.7MB|
|may_minihttp|0.79ms|0.47ms|0.02ms|8.10ms|323608.13|43.51MB/Sec|26.2MB|
|poem|0.97ms|0.48ms|0.02ms|6.04ms|264768.51|32.83MB/Sec|13.8MB|

`rewrk --threads 14 --connections 512 -d 60s -h http://127.0.0.1:3000`

| Framework Name | Latency Avg | Latency Stdev | Latency Min | Latency Max | Request Req/Sec | Transfer Rate | Max. Memory Usage |
|---|---|---|---|---|---|---|---|
|actix-web|1.65ms|0.97ms|0.02ms|16.15ms|310199.12|38.46MB/Sec|11.7MB|
|axum|1.83ms|0.91ms|0.02ms|10.85ms|279213.06|34.62MB/Sec|16.5MB|
|khttp|1.56ms|0.74ms|0.02ms|12.32ms|327909.88|44.09MB/Sec|1.7MB|
|may_minihttp|1.58ms|0.91ms|0.02ms|13.84ms|323415.27|43.49MB/Sec|43.5MB|
|poem|1.98ms|0.98ms|0.02ms|13.93ms|258851.83|32.09MB/Sec|22.1MB|

