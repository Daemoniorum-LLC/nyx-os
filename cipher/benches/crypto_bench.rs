//! Cryptographic operation benchmarks
//!
//! Run with: cargo bench -p nyx-cipher

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use nyx_cipher::crypto::{EncryptionKey, hash_password, generate_salt};

/// Benchmark encryption at various data sizes
fn bench_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption");
    let key = EncryptionKey::generate();

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let data = vec![0u8; size];
            b.iter(|| {
                key.encrypt(black_box(&data)).unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark decryption at various data sizes
fn bench_decryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("decryption");
    let key = EncryptionKey::generate();

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let plaintext = vec![0u8; size];
            let ciphertext = key.encrypt(&plaintext).unwrap();
            b.iter(|| {
                key.decrypt(black_box(&ciphertext)).unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark password hashing (intentionally slow - this is a security feature)
fn bench_password_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("password_hashing");
    group.sample_size(10); // Fewer samples since hashing is slow

    group.bench_function("hash_password", |b| {
        let password = "secure_password_123!@#";
        b.iter(|| {
            hash_password(black_box(password)).unwrap()
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
        let salt = generate_salt();
        b.iter(|| {
            EncryptionKey::derive_from_password(black_box(password), black_box(&salt)).unwrap()
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
