//! Cryptographic operation benchmarks
//!
//! Run with: cargo bench -p nyx-cipher

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};

// Note: These benchmarks reference the cipher crate's internals.
// In a real setup, you'd import from the crate:
// use nyx_cipher::crypto::{EncryptionKey, hash_password, verify_password};

/// Benchmark encryption at various data sizes
fn bench_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            // Would use actual EncryptionKey here
            let data = vec![0u8; size];
            b.iter(|| {
                black_box(&data);
                // key.encrypt(&data)
            });
        });
    }

    group.finish();
}

/// Benchmark decryption at various data sizes
fn bench_decryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("decryption");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let data = vec![0u8; size + 12 + 16]; // nonce + data + tag
            b.iter(|| {
                black_box(&data);
                // key.decrypt(&data)
            });
        });
    }

    group.finish();
}

/// Benchmark password hashing (intentionally slow)
fn bench_password_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("password_hashing");
    group.sample_size(10); // Fewer samples since hashing is slow

    group.bench_function("hash_password", |b| {
        let password = "secure_password_123!@#";
        b.iter(|| {
            black_box(password);
            // hash_password(password)
        });
    });

    group.finish();
}

/// Benchmark key derivation
fn bench_key_derivation(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_derivation");
    group.sample_size(10);

    group.bench_function("derive_from_password", |b| {
        let password = "my_secret_password";
        let salt = [0u8; 16];
        b.iter(|| {
            black_box((password, &salt));
            // EncryptionKey::derive_from_password(password, &salt)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_encryption,
    bench_decryption,
    bench_password_hashing,
    bench_key_derivation,
);

criterion_main!(benches);
