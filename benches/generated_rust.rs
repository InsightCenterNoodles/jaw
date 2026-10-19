use std::hint::black_box;
use std::io::{BufWriter, Cursor, Write};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

// Benchmark the checked-in output of the Rust generator for assets/example.jaw.
// Keeping this as a stable artifact makes results comparable across revisions.
#[allow(dead_code, clippy::all)]
#[rustfmt::skip]
#[path = "../assets/generated/rust/src/example.rs"]
mod generated;

#[allow(dead_code, clippy::all)]
#[rustfmt::skip]
#[path = "../assets/generated/rust/src/any.rs"]
mod generated_any;

use generated::*;

struct Fixture {
    name: &'static str,
    value: Root,
    encoded: Vec<u8>,
}

impl Fixture {
    fn new(name: &'static str, value: Root) -> Self {
        let mut encoded = Vec::new();
        value.jaw_write(&mut encoded).unwrap();
        Self {
            name,
            value,
            encoded,
        }
    }
}

fn small_root() -> Root {
    Root {
        name: ShortString(b"pod-one".to_vec()),
        var: MyVariant::MyPOD_1(MyPOD {
            a_thing: 10,
            b_thing: 20,
        }),
    }
}

fn complex_root() -> Root {
    let flags = MyFlagsUnpack {
        is_thing: 1,
        another_thing: 2,
        some_stuff: PlainEnum::F2,
    }
    .into();

    let list = MyPODFixedList([
        MyPOD {
            a_thing: 0,
            b_thing: 0,
        },
        MyPOD {
            a_thing: 1,
            b_thing: 10,
        },
        MyPOD {
            a_thing: 2,
            b_thing: 20,
        },
        MyPOD {
            a_thing: 3,
            b_thing: 30,
        },
        MyPOD {
            a_thing: 4,
            b_thing: 40,
        },
        MyPOD {
            a_thing: 5,
            b_thing: 50,
        },
        MyPOD {
            a_thing: 6,
            b_thing: 60,
        },
        MyPOD {
            a_thing: 7,
            b_thing: 70,
        },
    ]);

    let other_list = MyOtherPODDynList(vec![
        MyOtherPOD {
            first: MyPOD {
                a_thing: 9,
                b_thing: 0x0102_0304_0506_0708,
            },
            second: FixedString([9, 8, 7, 6]),
        },
        MyOtherPOD {
            first: MyPOD {
                a_thing: 10,
                b_thing: 0x1112_1314_1516_1718,
            },
            second: FixedString([1, 1, 2, 3]),
        },
        MyOtherPOD {
            first: MyPOD {
                a_thing: 11,
                b_thing: 0x2122_2324_2526_2728,
            },
            second: FixedString([4, 5, 6, 7]),
        },
    ]);

    Root {
        name: ShortString(b"complex".to_vec()),
        var: MyVariant::ComplexSeq_5(ComplexSeq {
            flags,
            list,
            other_list,
        }),
    }
}

#[derive(Default)]
struct CountingWriter {
    calls: usize,
    bytes: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.calls += 1;
        self.bytes += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn pack_array(count: usize) -> MyOtherPODDynList {
    MyOtherPODDynList(
        (0..count)
            .map(|i| MyOtherPOD {
                first: MyPOD {
                    a_thing: i as u8,
                    b_thing: i as u64,
                },
                second: FixedString([
                    i as u8,
                    i.wrapping_add(1) as u8,
                    i.wrapping_add(2) as u8,
                    i.wrapping_add(3) as u8,
                ]),
            })
            .collect(),
    )
}

fn generated_rust(c: &mut Criterion) {
    let fixtures = [
        Fixture::new("small", small_root()),
        Fixture::new("complex", complex_root()),
    ];

    let mut encode = c.benchmark_group("generated_rust/encode");
    for fixture in &fixtures {
        encode.throughput(Throughput::Bytes(fixture.encoded.len() as u64));
        let mut output = Vec::with_capacity(fixture.encoded.len());
        encode.bench_with_input(
            BenchmarkId::from_parameter(fixture.name),
            fixture,
            |b, fixture| {
                b.iter(|| {
                    output.clear();
                    black_box(&fixture.value)
                        .jaw_write(black_box(&mut output))
                        .unwrap();
                    black_box(&output);
                });
            },
        );
    }
    encode.finish();

    let mut decode = c.benchmark_group("generated_rust/decode");
    for fixture in &fixtures {
        decode.throughput(Throughput::Bytes(fixture.encoded.len() as u64));
        decode.bench_with_input(
            BenchmarkId::from_parameter(fixture.name),
            &fixture.encoded,
            |b, encoded| {
                b.iter(|| {
                    let mut input = Cursor::new(black_box(encoded.as_slice()));
                    let value = Root::jaw_read(&mut input).unwrap();
                    black_box(value);
                });
            },
        );
    }
    decode.finish();
}

fn byte_array_sizes(c: &mut Criterion) {
    use generated_any::{AnyBytes, JawRead, JawWrite};

    let fixtures: Vec<_> = [0usize, 64, 4 * 1024, 1024 * 1024]
        .into_iter()
        .map(|size| {
            let value = AnyBytes(vec![0xA5; size]);
            let mut encoded = Vec::new();
            value.jaw_write(&mut encoded).unwrap();
            (size, value, encoded)
        })
        .collect();

    let mut encode = c.benchmark_group("generated_rust/byte_array/encode_reused");
    for (size, value, encoded) in &fixtures {
        encode.throughput(Throughput::Bytes(encoded.len() as u64));
        let mut output = Vec::with_capacity(encoded.len());
        encode.bench_with_input(BenchmarkId::from_parameter(size), value, |b, value| {
            b.iter(|| {
                output.clear();
                black_box(value).jaw_write(black_box(&mut output)).unwrap();
                black_box(&output);
            });
        });
    }
    encode.finish();

    let mut decode = c.benchmark_group("generated_rust/byte_array/decode_owned");
    for (size, _, encoded) in &fixtures {
        decode.throughput(Throughput::Bytes(encoded.len() as u64));
        decode.bench_with_input(BenchmarkId::from_parameter(size), encoded, |b, encoded| {
            b.iter(|| {
                let mut input = Cursor::new(black_box(encoded.as_slice()));
                black_box(AnyBytes::jaw_read(&mut input).unwrap());
            });
        });
    }
    decode.finish();

    let (_, representative, encoded) = &fixtures[2];
    let mut counter = CountingWriter::default();
    representative.jaw_write(&mut counter).unwrap();
    assert_eq!(counter.calls, 2, "byte array should write count and body");
    assert_eq!(counter.bytes, encoded.len());

    let mut modes = c.benchmark_group("generated_rust/byte_array/writer_modes_4k");
    modes.throughput(Throughput::Bytes(encoded.len() as u64));
    modes.bench_function("fresh_vec", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            black_box(representative)
                .jaw_write(black_box(&mut output))
                .unwrap();
            black_box(output);
        });
    });
    let mut output = BufWriter::new(Vec::with_capacity(encoded.len()));
    modes.bench_function("buffered_vec", |b| {
        b.iter(|| {
            output.get_mut().clear();
            black_box(representative)
                .jaw_write(black_box(&mut output))
                .unwrap();
            output.flush().unwrap();
            black_box(output.get_ref());
        });
    });
    modes.finish();
}

fn pack_array_sizes(c: &mut Criterion) {
    let fixtures: Vec<_> = [1usize, 16, 1024, u16::MAX as usize]
        .into_iter()
        .map(|size| {
            let value = pack_array(size);
            let mut encoded = Vec::new();
            value.jaw_write(&mut encoded).unwrap();
            (size, value, encoded)
        })
        .collect();

    let mut encode = c.benchmark_group("generated_rust/pack_array/encode_reused");
    for (size, value, encoded) in &fixtures {
        encode.throughput(Throughput::Bytes(encoded.len() as u64));
        let mut output = Vec::with_capacity(encoded.len());
        encode.bench_with_input(BenchmarkId::from_parameter(size), value, |b, value| {
            b.iter(|| {
                output.clear();
                black_box(value).jaw_write(black_box(&mut output)).unwrap();
                black_box(&output);
            });
        });
    }
    encode.finish();

    let mut decode = c.benchmark_group("generated_rust/pack_array/decode_owned");
    for (size, _, encoded) in &fixtures {
        decode.throughput(Throughput::Bytes(encoded.len() as u64));
        decode.bench_with_input(BenchmarkId::from_parameter(size), encoded, |b, encoded| {
            b.iter(|| {
                let mut input = Cursor::new(black_box(encoded.as_slice()));
                black_box(MyOtherPODDynList::jaw_read(&mut input).unwrap());
            });
        });
    }
    decode.finish();

    let (_, representative, encoded) = &fixtures[2];
    let mut counter = CountingWriter::default();
    representative.jaw_write(&mut counter).unwrap();
    assert_eq!(
        counter.calls, 2,
        "pack array should bulk-write its count and body"
    );
    assert_eq!(counter.bytes, encoded.len());

    let mut modes = c.benchmark_group("generated_rust/pack_array/writer_modes_1k");
    modes.throughput(Throughput::Bytes(encoded.len() as u64));
    modes.bench_function("fresh_vec", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            black_box(representative)
                .jaw_write(black_box(&mut output))
                .unwrap();
            black_box(output);
        });
    });
    let mut output = BufWriter::new(Vec::with_capacity(encoded.len()));
    modes.bench_function("buffered_vec", |b| {
        b.iter(|| {
            output.get_mut().clear();
            black_box(representative)
                .jaw_write(black_box(&mut output))
                .unwrap();
            output.flush().unwrap();
            black_box(output.get_ref());
        });
    });
    modes.finish();
}

criterion_group!(benches, generated_rust, byte_array_sizes, pack_array_sizes);
criterion_main!(benches);
