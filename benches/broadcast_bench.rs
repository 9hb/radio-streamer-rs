use bytes::Bytes;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::sleep;

async fn run_benchmark(
    num_listeners: usize,
    channel_capacity: usize,
    chunk_count: usize,
    paced_ms: Option<u64>,
) {
    let (tx, _) = broadcast::channel::<Bytes>(channel_capacity);
    let chunk_size = 1024; // 1 KB typical audio chunk
    let test_chunk = Bytes::from(vec![0xAA; chunk_size]);

    let total_received = Arc::new(AtomicU64::new(0));
    let total_lagged = Arc::new(AtomicUsize::new(0));
    let ready_barrier = Arc::new(tokio::sync::Barrier::new(num_listeners + 1));

    let mut handles = Vec::with_capacity(num_listeners);

    for _ in 0..num_listeners {
        let mut rx = tx.subscribe();
        let r_count = Arc::clone(&total_received);
        let l_count = Arc::clone(&total_lagged);
        let b = Arc::clone(&ready_barrier);

        let handle = tokio::spawn(async move {
            b.wait().await;
            loop {
                match rx.recv().await {
                    Ok(_) => {
                        r_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        l_count.fetch_add(missed as usize, Ordering::Relaxed);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
        handles.push(handle);
    }

    ready_barrier.wait().await;

    let start = Instant::now();

    for _ in 0..chunk_count {
        let _ = tx.send(test_chunk.clone());
        if let Some(delay) = paced_ms {
            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            } else {
                tokio::task::yield_now().await;
            }
        } else {
            tokio::task::yield_now().await;
        }
    }

    drop(tx);

    for h in handles {
        let _ = h.await;
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let total_msgs = total_received.load(Ordering::Relaxed);
    let lagged_msgs = total_lagged.load(Ordering::Relaxed);
    let expected_msgs = (num_listeners * chunk_count) as u64;

    let total_bytes = total_msgs * chunk_size as u64;
    let mb_transferred = total_bytes as f64 / (1024.0 * 1024.0);
    let throughput_mb_s = mb_transferred / elapsed_secs.max(0.0001);
    let msgs_per_sec = total_msgs as f64 / elapsed_secs.max(0.0001);
    let delivery_rate = if expected_msgs > 0 {
        (total_msgs as f64 / expected_msgs as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Listeners: {:>5} | Time: {:>5.2}s | Data: {:>7.2} MB | Throughput: {:>8.2} MB/s ({:>8.0} msg/s) | Delivery: {:>5.1}% | Lagged: {:>5}",
        num_listeners,
        elapsed_secs,
        mb_transferred,
        throughput_mb_s,
        msgs_per_sec,
        delivery_rate,
        lagged_msgs
    );
}

#[tokio::main]
async fn main() {
    println!();
    println!(
        "================================================================================================="
    );
    println!(
        "             RADIO STREAMER - BROADCAST CHANNEL CONCURRENCY BENCHMARK                            "
    );
    println!(
        "================================================================================================="
    );
    println!("Scenario 1: Real-World Paced Audio Streaming (5ms pacing ~ 200KB/s per stream)");
    println!(
        "-------------------------------------------------------------------------------------------------"
    );
    for &listeners in &[10, 50, 100, 500, 1000, 2500, 5000] {
        run_benchmark(listeners, 128, 50, Some(5)).await;
    }

    println!();
    println!("Scenario 2: Maximum Saturating Throughput (Zero-Delay Stress Test)");
    println!(
        "-------------------------------------------------------------------------------------------------"
    );
    for &listeners in &[10, 50, 100, 500, 1000, 2500, 5000] {
        run_benchmark(listeners, 128, 200, None).await;
    }

    println!(
        "================================================================================================="
    );
    println!("Benchmark completed successfully.");
    println!();
}
