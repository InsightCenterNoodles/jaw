use std::fs;
use std::io::{self, Cursor};
use std::path::PathBuf;

#[allow(dead_code)]
mod example;

fn demand(cond: bool, label: &str) {
    if !cond {
        eprintln!("Error: {}", label);
        std::process::exit(1);
    }
}

fn encode_expected() -> Vec<u8> {
    use example::write::*;

    let mut expected: Vec<u8> = Vec::new();

    // 1) Variant = MyPOD
    {
        let name = b"pod-one";

        write_Root(
            &mut expected,
            &Root {
                name: name,
                var: MyVariant::MyPOD_1(&MyPOD {
                    a_thing: 10,
                    b_thing: 20,
                }),
            },
        )
        .unwrap();
    }

    // 2) Variant = MyOtherPOD
    {
        write_Root(
            &mut expected,
            &Root {
                name: b"",
                var: MyVariant::MyOtherPOD_2(&MyOtherPOD {
                    first: MyPOD {
                        a_thing: 1,
                        b_thing: 0x1122334455667788,
                    },
                    second: [1u8, 2u8, 3u8, 4u8],
                }),
            },
        )
        .unwrap();
    }

    // 3) Variant = void
    {
        write_Root(
            &mut expected,
            &Root {
                name: b"void",
                var: MyVariant::void_3,
            },
        )
        .unwrap();
    }

    // 4) Variant = SmallSeq with floats
    {
        write_Root(
            &mut expected,
            &Root {
                name: b"floats",
                var: MyVariant::SmallSeq_4(&SmallSeq {
                    list: &[1.0, 2.5, -3.25, 0.0],
                }),
            },
        )
        .unwrap();
    }

    // 5) Another MyPOD with larger values
    {
        write_Root(
            &mut expected,
            &Root {
                name: b"pod-two",
                var: MyVariant::MyPOD_1(&MyPOD {
                    a_thing: 255,
                    b_thing: 0xCAFEBABECAFED00D,
                }),
            },
        )
        .unwrap();
    }

    // 6) Another SmallSeq, longer
    {
        let vec: Vec<_> = (0..10).map(|x| (x as f32) * 0.5).collect();
        write_Root(
            &mut expected,
            &Root {
                name: b"seq-long",
                var: MyVariant::SmallSeq_4(&SmallSeq { list: &vec }),
            },
        )
        .unwrap();
    }

    expected
}

fn make_expected() -> Vec<example::read::Root> {
    use example::read::*;

    let mut expected: Vec<_> = Vec::new();

    // 1) Variant = MyPOD
    {
        expected.push(Root {
            name: b"pod-one".into(),
            var: MyVariant::MyPOD_1(MyPOD {
                a_thing: 10,
                b_thing: 20,
            }),
        });
    }

    // 2) Variant = MyOtherPOD
    {
        expected.push(Root {
            name: b"".into(),
            var: MyVariant::MyOtherPOD_2(MyOtherPOD {
                first: MyPOD {
                    a_thing: 1,
                    b_thing: 0x1122334455667788,
                },
                second: [1u8, 2u8, 3u8, 4u8],
            }),
        });
    }

    // 3) Variant = void
    {
        expected.push(Root {
            name: b"void".into(),
            var: MyVariant::void_3,
        });
    }

    // 4) Variant = SmallSeq with floats
    {
        expected.push(Root {
            name: b"floats".into(),
            var: MyVariant::SmallSeq_4(SmallSeq {
                list: [1.0, 2.5, -3.25, 0.0].into(),
            }),
        });
    }

    // 5) Another MyPOD with larger values
    {
        expected.push(Root {
            name: b"pod-two".into(),
            var: MyVariant::MyPOD_1(MyPOD {
                a_thing: 255,
                b_thing: 0xCAFEBABECAFED00D,
            }),
        });
    }

    // 6) Another SmallSeq, longer
    {
        let vec: Vec<_> = (0..10).map(|x| (x as f32) * 0.5).collect();
        expected.push(Root {
            name: b"seq-long".into(),
            var: MyVariant::SmallSeq_4(SmallSeq { list: vec }),
        });
    }

    expected
}

fn decode_all(buf: &mut [u8], count: usize) -> io::Result<Vec<example::read::Root>> {
    use example::read::*;
    println!("Decode all {count}");
    let mut r = Cursor::new(buf);
    let mut out: Vec<Root> = Vec::with_capacity(count);
    for i in 0..count {
        out.push(
            read_Root(&mut r)
                .inspect(|x| println!("Item: {x:?}"))
                .inspect_err(|x| eprintln!("Unable to read root {i} {x}"))?,
        );
    }
    Ok(out)
}

fn compare_expected_actual(expected: &[example::read::Root], actual: &[example::read::Root]) {
    use example::read::*;
    demand(expected.len() == actual.len(), "size matches");
    for (i, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
        demand(a.name == e.name, &format!("name matches at {}", i));
        // Compare variant kind
        match (&e.var, &a.var) {
            (MyVariant::MyPOD_1(ep), MyVariant::MyPOD_1(ap)) => {
                demand(ap.a_thing == ep.a_thing, &format!("MyPOD.a_thing at {}", i));
                demand(ap.b_thing == ep.b_thing, &format!("MyPOD.b_thing at {}", i));
            }
            (MyVariant::MyOtherPOD_2(eo), MyVariant::MyOtherPOD_2(ao)) => {
                demand(
                    ao.first.a_thing == eo.first.a_thing,
                    &format!("MyOtherPOD.first.a_thing at {}", i),
                );
                demand(
                    ao.first.b_thing == eo.first.b_thing,
                    &format!("MyOtherPOD.first.b_thing at {}", i),
                );
                for j in 0..4 {
                    demand(
                        ao.second[j] == eo.second[j],
                        &format!("MyOtherPOD.second[{}] at {}", j, i),
                    );
                }
            }
            (MyVariant::void_3, MyVariant::void_3) => {
                // void payload, nothing to compare
            }
            (MyVariant::SmallSeq_4(es), MyVariant::SmallSeq_4(as_)) => {
                demand(
                    as_.list.len() == es.list.len(),
                    &format!("SmallSeq.size at {}", i),
                );
                for (j, (x, y)) in as_.list.iter().zip(es.list.iter()).enumerate() {
                    demand(*x == *y, &format!("SmallSeq.value[{}] at {}", j, i));
                }
            }
            _ => demand(false, &format!("variant tag matches at {}", i)),
        }
    }
}

fn usage() {
    eprintln!("Usage: jaw_rust [--dump PATH] [--read PATH]");
}

fn main() -> io::Result<()> {
    println!("Rust test harness");
    let mut dump_path: Option<PathBuf> = None;
    let mut read_path: Option<PathBuf> = None;

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--dump" if i + 1 < args.len() => {
                dump_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--read" if i + 1 < args.len() => {
                read_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => {
                usage();
                return Ok(());
            }
        }
    }

    let expected = make_expected();

    if let Some(path) = read_path {
        let mut data = fs::read(&path)?;
        eprintln!("Encoded size {}", data.len());
        let actual = decode_all(&mut data, expected.len())?;
        compare_expected_actual(&expected, &actual);
        println!("Verified dump ok ({} messages)", expected.len());
    }

    // Local roundtrip

    let mut expected_bytes = encode_expected();

    eprintln!("Encoded size {}", expected_bytes.len());
    let actual = decode_all(&mut expected_bytes, expected.len())?;
    compare_expected_actual(&make_expected(), &actual);
    println!("All checks passed ({} messages)", expected.len());

    if let Some(path) = dump_path {
        fs::write(&path, &expected_bytes)?;
        println!("Wrote {} bytes to {}", expected_bytes.len(), path.display());
    }

    Ok(())
}
