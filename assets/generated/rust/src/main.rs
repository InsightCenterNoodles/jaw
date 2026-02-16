use std::fs;
use std::io::{self, Cursor};
use std::path::PathBuf;

#[allow(dead_code)]
mod example;

#[allow(dead_code)]
mod any;

use example::*;

fn demand(cond: bool, label: &str) {
    if !cond {
        eprintln!("Error: {}", label);
        std::process::exit(1);
    }
}

fn encode_expected() -> Vec<u8> {
    let mut expected: Vec<u8> = Vec::new();

    // 1) Variant = MyPOD
    {
        let name = b"pod-one";

        RootView {
            name: ShortStringView(name),
            var: MyVariantView::MyPOD_1(&MyPOD {
                a_thing: 10,
                b_thing: 20,
            }),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 2) Variant = MyOtherPOD
    {
        Root {
            name: ShortString(b"".to_vec()),
            var: MyVariant::MyOtherPOD_2(MyOtherPOD {
                first: MyPOD {
                    a_thing: 1,
                    b_thing: 0x1122334455667788,
                },
                second: example::FixedString([1u8, 2u8, 3u8, 4u8]),
            }),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 3) Variant = void
    {
        RootView {
            name: example::ShortStringView(b"void"),
            var: MyVariantView::void_3,
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 4) Variant = SmallSeq with floats
    {
        Root {
            name: ShortString(b"floats".to_vec()),
            var: MyVariant::SmallSeq_4(SmallSeq {
                list: Data([1.0, 2.5, -3.25, 0.0].to_vec()),
            }),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 5) Another MyPOD with larger values
    {
        RootView {
            name: ShortStringView(b"pod-two"),
            var: MyVariantView::MyPOD_1(&MyPOD {
                a_thing: 255,
                b_thing: 0xCAFEBABECAFED00D,
            }),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 6) Another SmallSeq, longer
    {
        let vec: Vec<_> = (0..10).map(|x| (x as f32) * 0.5).collect();
        RootView {
            name: ShortStringView(b"seq-long"),
            var: MyVariantView::SmallSeqView_4(&SmallSeqView {
                list: DataView(&vec),
            }),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    // 7) Variant = ComplexSeq (bitfields, fixed arrays of POD, dyn arrays of POD)
    {
        let fixed_list: [MyPOD; 8] = [
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
        ];
        let others = vec![
            MyOtherPOD {
                first: MyPOD {
                    a_thing: 9,
                    b_thing: 0x0102030405060708,
                },
                second: example::FixedString([9u8, 8u8, 7u8, 6u8]),
            },
            MyOtherPOD {
                first: MyPOD {
                    a_thing: 10,
                    b_thing: 0x1112131415161718,
                },
                second: example::FixedString([1u8, 1u8, 2u8, 3u8]),
            },
            MyOtherPOD {
                first: MyPOD {
                    a_thing: 11,
                    b_thing: 0x2122232425262728,
                },
                second: example::FixedString([4u8, 5u8, 6u8, 7u8]),
            },
        ];
        let flags: MyFlags = MyFlagsUnpack {
            is_thing: 1,
            another_thing: 2,
            some_stuff: PlainEnum::F2,
        }
        .into();
        let seq = ComplexSeq {
            flags,
            list: MyPODFixedList(fixed_list),
            other_list: MyOtherPODDynList(others),
        };
        Root {
            name: ShortString(b"complex".to_vec()),
            var: MyVariant::ComplexSeq_5(seq),
        }
        .jaw_write(&mut expected)
        .unwrap();
    }

    expected
}

fn make_expected() -> Vec<Root> {
    let mut expected: Vec<_> = Vec::new();

    // 1) Variant = MyPOD
    {
        expected.push(Root {
            name: ShortString(b"pod-one".into()),
            var: MyVariant::MyPOD_1(MyPOD {
                a_thing: 10,
                b_thing: 20,
            }),
        });
    }

    // 2) Variant = MyOtherPOD
    {
        expected.push(Root {
            name: ShortString(b"".into()),
            var: MyVariant::MyOtherPOD_2(MyOtherPOD {
                first: MyPOD {
                    a_thing: 1,
                    b_thing: 0x1122334455667788,
                },
                second: example::FixedString([1u8, 2u8, 3u8, 4u8]),
            }),
        });
    }

    // 3) Variant = void
    {
        expected.push(Root {
            name: ShortString(b"void".into()),
            var: MyVariant::void_3,
        });
    }

    // 4) Variant = SmallSeq with floats
    {
        expected.push(Root {
            name: ShortString(b"floats".into()),
            var: MyVariant::SmallSeq_4(SmallSeq {
                list: Data([1.0, 2.5, -3.25, 0.0].into()),
            }),
        });
    }

    // 5) Another MyPOD with larger values
    {
        expected.push(Root {
            name: ShortString(b"pod-two".into()),
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
            name: ShortString(b"seq-long".into()),
            var: MyVariant::SmallSeq_4(SmallSeq {
                list: example::Data(vec),
            }),
        });
    }

    // 7) Variant = ComplexSeq (bitfields, fixed arrays of POD, dyn arrays of POD)
    {
        expected.push(Root {
            name: ShortString(b"complex".into()),
            var: MyVariant::ComplexSeq_5(ComplexSeq {
                flags: MyFlagsUnpack {
                    is_thing: 1,
                    another_thing: 2,
                    some_stuff: PlainEnum::F2,
                }
                .into(),
                list: example::MyPODFixedList([
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
                ]),
                other_list: MyOtherPODDynList(vec![
                    MyOtherPOD {
                        first: MyPOD {
                            a_thing: 9,
                            b_thing: 0x0102030405060708,
                        },
                        second: example::FixedString([9u8, 8u8, 7u8, 6u8]),
                    },
                    MyOtherPOD {
                        first: MyPOD {
                            a_thing: 10,
                            b_thing: 0x1112131415161718,
                        },
                        second: example::FixedString([1u8, 1u8, 2u8, 3u8]),
                    },
                    MyOtherPOD {
                        first: MyPOD {
                            a_thing: 11,
                            b_thing: 0x2122232425262728,
                        },
                        second: example::FixedString([4u8, 5u8, 6u8, 7u8]),
                    },
                ]),
            }),
        });
    }

    expected
}

fn decode_all(buf: &mut [u8], count: usize) -> io::Result<Vec<Root>> {
    println!("Decode all {count}");
    let mut r = Cursor::new(buf);
    let mut out: Vec<Root> = Vec::with_capacity(count);
    for i in 0..count {
        out.push(
            Root::jaw_read(&mut r)
                .inspect(|x| println!("Item: {x:?}"))
                .inspect_err(|x| eprintln!("Unable to read root {i} {x}"))?,
        );
    }
    Ok(out)
}

fn compare_expected_actual(expected: &[Root], actual: &[Root]) {
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
                        ao.second.0[j] == eo.second.0[j],
                        &format!("MyOtherPOD.second[{}] at {}", j, i),
                    );
                }
            }
            (MyVariant::void_3, MyVariant::void_3) => {
                // void payload, nothing to compare
            }
            (MyVariant::SmallSeq_4(es), MyVariant::SmallSeq_4(as_)) => {
                demand(
                    as_.list.0.len() == es.list.0.len(),
                    &format!("SmallSeq.size at {}", i),
                );
                for (j, (x, y)) in as_.list.0.iter().zip(es.list.0.iter()).enumerate() {
                    demand(*x == *y, &format!("SmallSeq.value[{}] at {}", j, i));
                }
            }
            (MyVariant::ComplexSeq_5(es), MyVariant::ComplexSeq_5(as_)) => {
                demand(
                    as_.flags.is_thing() == es.flags.is_thing(),
                    &format!("ComplexSeq.flags.is_thing at {}", i),
                );
                demand(
                    as_.flags.another_thing() == es.flags.another_thing(),
                    &format!("ComplexSeq.flags.another_thing at {}", i),
                );
                demand(
                    as_.flags.some_stuff().unwrap() == es.flags.some_stuff().unwrap(),
                    &format!("ComplexSeq.flags.some_stuff at {}", i),
                );
                for (j, (x, y)) in as_.list.0.iter().zip(es.list.0.iter()).enumerate() {
                    demand(
                        x.a_thing == y.a_thing,
                        &format!("ComplexSeq.list[{}].a_thing at {}", j, i),
                    );
                    demand(
                        x.b_thing == y.b_thing,
                        &format!("ComplexSeq.list[{}].b_thing at {}", j, i),
                    );
                }
                demand(
                    as_.other_list.0.len() == es.other_list.0.len(),
                    &format!("ComplexSeq.other_list.size at {}", i),
                );
                for (j, (x, y)) in as_
                    .other_list
                    .0
                    .iter()
                    .zip(es.other_list.0.iter())
                    .enumerate()
                {
                    demand(
                        x.first.a_thing == y.first.a_thing,
                        &format!("ComplexSeq.other_list[{}].first.a_thing at {}", j, i),
                    );
                    demand(
                        x.first.b_thing == y.first.b_thing,
                        &format!("ComplexSeq.other_list[{}].first.b_thing at {}", j, i),
                    );
                    for k in 0..4 {
                        demand(
                            x.second.0[k] == y.second.0[k],
                            &format!("ComplexSeq.other_list[{}].second[{}] at {}", j, k, i),
                        );
                    }
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

    // Extra self-checks for DSL features not representable via Root
    {
        let mut r = Cursor::new(vec![2u8]);
        let got = BetterEnum::jaw_read(&mut r)?;
        demand(got == BetterEnum::DEFAULT, "BetterEnum default");
    }

    if let Some(path) = dump_path {
        fs::write(&path, &expected_bytes)?;
        println!("Wrote {} bytes to {}", expected_bytes.len(), path.display());
    }

    Ok(())
}
