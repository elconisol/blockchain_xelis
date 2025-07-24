use criterion::{black_box, criterion_group, criterion_main, Criterion};
use curve25519_dalek::Scalar;
use xelis_common::crypto::KeyPair;

/// Benchmarks Homomorphic Encryption operations used in the XELIS network,
/// based on the Twisted ElGamal encryption scheme.
fn bench_homomorphic_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("homomorphic_encryption");

    let keypair = KeyPair::new();
    let public_key = keypair.get_public_key();
    let amount = 100u64;
    let scalar = Scalar::from(50u64);

    // Prepare ciphertexts
    let ct1 = public_key.encrypt(amount);
    let ct2 = public_key.encrypt(scalar);

    group.bench_function("ciphertext + ciphertext", |b| {
        b.iter(|| {
            let _ = black_box(ct1.clone() + ct2.clone());
        });
    });

    group.bench_function("ciphertext + scalar", |b| {
        b.iter(|| {
            let _ = black_box(ct1.clone() + scalar);
        });
    });

    group.bench_function("ciphertext - ciphertext", |b| {
        b.iter(|| {
            let _ = black_box(ct1.clone() - ct2.clone());
        });
    });

    group.bench_function("ciphertext - scalar", |b| {
        b.iter(|| {
            let _ = black_box(ct1.clone() - scalar);
        });
    });

    group.finish();
}

criterion_group!(he_benches, bench_homomorphic_operations);
criterion_main!(he_benches);
