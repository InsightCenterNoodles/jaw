use std::fs;
use std::io::{self, Cursor};
use std::path::PathBuf;

// Include generated Rust module for assets/basic.jaw
include!(concat!(env!("OUT_DIR"), "/basic.rs"));
use basic::*;

fn demand(cond: bool, label: &str) {
    if !cond {
        eprintln!("Error: {}", label);
        std::process::exit(1);
    }
}

fn build_expected() -> Vec<Root> {
    let mut expected: Vec<Root> = Vec::new();

    // 1) Variant = MyPOD
    {
        let mut r = Root {
            name: Vec::new(),
            var: MyVariant::Alt0(MyPOD {
                a_thing: 0,
                b_thing: 0,
            }),
        };
        r.name = b"pod-one".to_vec();
        let pod = MyPOD {
            a_thing: 10,
            b_thing: 20,
        };
        r.var = MyVariant::Alt0(pod);
        expected.push(r);
    }

    // 2) Variant = MyOtherPOD
    {
        let mut r = Root {
            name: Vec::new(),
            var: MyVariant::Alt0(MyPOD {
                a_thing: 0,
                b_thing: 0,
            }),
        };
        r.name = b"".to_vec();
        let first = MyPOD {
            a_thing: 1,
            b_thing: 0x1122_3344_5566_7788u64,
        };
        let second: [u8; 4] = [1, 2, 3, 4];
        let other = MyOtherPOD { first, second };
        r.var = MyVariant::Alt1(other);
        expected.push(r);
    }

    // 3) Variant = SmallSeq with floats
    {
        let mut r = Root {
            name: Vec::new(),
            var: MyVariant::Alt0(MyPOD {
                a_thing: 0,
                b_thing: 0,
            }),
        };
        r.name = b"floats".to_vec();
        let seq = SmallSeq {
            list: vec![1.0, 2.5, -3.25, 0.0],
        };
        r.var = MyVariant::DefaultAlt(seq); // default alt is tag=2
        expected.push(r);
    }

    // 4) Another MyPOD with larger values
    {
        let mut r = Root {
            name: Vec::new(),
            var: MyVariant::Alt0(MyPOD {
                a_thing: 0,
                b_thing: 0,
            }),
        };
        r.name = b"pod-two".to_vec();
        let pod = MyPOD {
            a_thing: 255,
            b_thing: 0xCAFEBABECAFED00Du64,
        };
        r.var = MyVariant::Alt0(pod);
        expected.push(r);
    }

    // 5) Another SmallSeq, longer
    {
        let mut r = Root {
            name: Vec::new(),
            var: MyVariant::Alt0(MyPOD {
                a_thing: 0,
                b_thing: 0,
            }),
        };
        r.name = b"seq-long".to_vec();
        let mut list = Vec::new();
        for i in 0..10 {
            list.push((i as f32) * 0.5);
        }
        r.var = MyVariant::DefaultAlt(SmallSeq { list });
        expected.push(r);
    }

    expected
}

fn encode_all(msgs: &[Root]) -> io::Result<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    for r in msgs {
        let view = RootView {
            name: &r.name,
            var: &r.var,
        };
        write_RootView(&mut out, &view)
            .inspect_err(|x| eprintln!("Unable to write root view {x}"))?;
    }
    Ok(out)
}

fn decode_all(buf: &[u8], count: usize) -> io::Result<Vec<Root>> {
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

fn compare_expected_actual(expected: &[Root], actual: &[Root]) {
    demand(expected.len() == actual.len(), "size matches");
    for (i, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
        demand(a.name == e.name, &format!("name matches at {}", i));
        // Compare variant kind
        match (&e.var, &a.var) {
            (MyVariant::Alt0(ep), MyVariant::Alt0(ap)) => {
                demand(ap.a_thing == ep.a_thing, &format!("MyPOD.a_thing at {}", i));
                demand(ap.b_thing == ep.b_thing, &format!("MyPOD.b_thing at {}", i));
            }
            (MyVariant::Alt1(eo), MyVariant::Alt1(ao)) => {
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
            (MyVariant::DefaultAlt(es), MyVariant::DefaultAlt(as_)) => {
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

    let expected = build_expected();

    if let Some(path) = read_path {
        let data = fs::read(&path)?;
        eprintln!("Encoded size {}", data.len());
        let actual = decode_all(&data, expected.len())?;
        compare_expected_actual(&expected, &actual);
        // Ensure fully consumed
        let mut r = Cursor::new(&data);
        for _ in 0..expected.len() {
            let _ = read_Root(&mut r)?;
        }
        demand(r.position() as usize == data.len(), "buffer fully consumed");
        println!("Verified dump ok ({} messages)", expected.len());
    }

    // Local roundtrip
    let data = encode_all(&expected)?;

    eprintln!("Encoded size {}", data.len());
    let actual = decode_all(&data, expected.len())?;
    compare_expected_actual(&expected, &actual);
    println!("All checks passed ({} messages)", expected.len());

    if let Some(path) = dump_path {
        fs::write(&path, &data)?;
        println!("Wrote {} bytes to {}", data.len(), path.display());
    }

    Ok(())
}
