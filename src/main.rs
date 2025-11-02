use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TypeID(u32);

#[derive(Debug)]
enum Primitive {
    I8,
    I16,
    I32,
    I64,

    U8,
    U16,
    U32,
    U64,

    F32,
    F64,
}

#[derive(Debug)]
struct Pack {
    members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
struct Enum {
    ty: Primitive,
    members: Vec<(String, u64)>,
    default: Option<(String, u64)>,
}

#[derive(Debug)]
struct Bitfld {
    ty: TypeID,
    members: Vec<(String, TypeID, RangeInclusive<u32>)>,
}

#[derive(Debug)]
struct Variant {
    ty: TypeID,
    members: Vec<(u64, TypeID)>,
    default: Option<(u64, TypeID)>,
}

#[derive(Debug)]
struct Sequence {
    members: Vec<(String, TypeID)>,
}

#[derive(Debug)]
enum ArrayKind {
    Dynamic {
        size_type: TypeID,
        value_type: TypeID,
    },
    Fixed {
        count: u64,
        value_type: TypeID,
    },
}

#[derive(Debug)]
enum Type {
    Primitive(Primitive),
    Alias(Box<Type>),
    Pack(Pack),
    Enum(Enum),
    Bitfld(Bitfld),
    Variant(Variant),
    Sequence(Sequence),
    DynamicArray(ArrayKind),
    FixedArray(ArrayKind),
}

const KW_PACK: &str = "pack";
const KW_ENUM: &str = "enum";
const KW_BITS: &str = "bits";
const KW_VARIANT: &str = "variant";
const KW_SEQ: &str = "seq";

#[derive(Debug)]
struct Module {
    type_map: HashMap<String, TypeID>,
    types: HashMap<TypeID, Type>,
    last_tid: u32,
}

impl Module {
    fn from_file(path: &Path) -> std::io::Result<Self> {
        let mut reader = Reader::new(path)?;

        let mut module = Module {
            type_map: Default::default(),
            types: Default::default(),
            last_tid: 0,
        };

        module.add_builins();

        type FPtr = fn(&mut Module, &mut Reader) -> std::io::Result<()>;

        //let function_map = HashMap::<_, FPtr>::from([(KW_PACK, Module::parse_pack)]);

        let mut function_map = HashMap::<_, FPtr>::default();

        function_map.insert(KW_PACK, Module::parse_pack);
        function_map.insert(KW_ENUM, Module::parse_enum);
        function_map.insert(KW_BITS, Module::parse_bits);
        function_map.insert(KW_VARIANT, Module::parse_variant);
        function_map.insert(KW_SEQ, Module::parse_seq);

        let mut next_keyword = String::new();

        loop {
            {
                let Some(line) = reader.advance()? else {
                    break;
                };

                dbg!(line);

                let mut parts = line.split_ascii_whitespace();

                let Some(keyword) = parts.next() else {
                    continue;
                };

                // bit silly no?
                next_keyword.clear();
                next_keyword.extend(keyword.chars());
            }

            let function = {
                function_map
                    .get(next_keyword.as_str())
                    .ok_or_else(|| reader.make_error(format!("{next_keyword} is not a keyword")))?
            };

            function(&mut module, &mut reader)?;
        }

        Ok(module)
    }

    fn provision_type_id(&mut self, mut name: &str) -> std::io::Result<TypeID> {
        name = name.trim();

        if let Some(x) = name.chars().next() {
            if !x.is_alphabetic() {
                return Err(std::io::Error::other("types must start with alpha"));
            }
        }

        for c in name.chars() {
            if c.is_whitespace() {
                return Err(std::io::Error::other("types must not contain whitespace"));
            }
        }

        Ok(*self.type_map.entry(name.to_string()).or_insert_with(|| {
            let ret = TypeID(self.last_tid);
            self.last_tid += 1;
            ret
        }))
    }

    fn lookup(&self, t: TypeID) -> Option<&Type> {
        self.types.get(&t)
    }

    fn add_builins(&mut self) {
        let mut add_prim = |name: &str, p: Primitive| {
            let id = self.provision_type_id(name).unwrap();
            self.types.insert(id, Type::Primitive(p));
        };

        add_prim("u8", Primitive::U8);
        add_prim("u16", Primitive::U16);
        add_prim("u32", Primitive::U32);
        add_prim("u64", Primitive::U64);

        add_prim("i8", Primitive::I8);
        add_prim("i16", Primitive::I16);
        add_prim("i32", Primitive::I32);
        add_prim("i64", Primitive::I64);

        add_prim("f32", Primitive::F32);
        add_prim("f64", Primitive::F64);
    }

    fn parse_decl_head(
        module: &mut Module,
        reader: &mut Reader,
    ) -> std::io::Result<(TypeID, Option<TypeID>)> {
        let tid = {
            let Some(x) = reader
                .current_line()
                .split_ascii_whitespace()
                .skip(1)
                .next()
            else {
                return Err(reader.make_error("missing pack name"));
            };

            module.provision_type_id(x)?
        };

        let op = reader
            .current_line()
            .split_once(':')
            .and_then(|x| if x.1.is_empty() { None } else { Some(x.1) })
            .map(|x| module.provision_type_id(x))
            .transpose()?;

        Ok((tid, op))
    }

    fn parse_body<F, G>(
        module: &mut Module,
        reader: &mut Reader,
        mut f: F,
        mut def: G,
    ) -> std::io::Result<()>
    where
        F: FnMut(&mut Module, &Reader, &str) -> std::io::Result<()>,
        G: FnMut(&mut Module, &Reader, &str) -> std::io::Result<()>,
    {
        loop {
            let Some(line) = reader.advance()? else {
                break;
            };

            let line = line.trim();

            let first = line.chars().next();

            match first {
                Some('-') => {
                    f(module, &reader, line.trim_start())?;
                }
                Some('=') => {
                    def(module, &reader, line.trim_start())?;
                }
                _ => {
                    break;
                }
            }
        }

        Ok(())
    }

    fn parse_pack(module: &mut Module, reader: &mut Reader) -> std::io::Result<()> {
        let (tid, sub) = Module::parse_decl_head(module, reader)?;

        if sub.is_some() {
            return Err(reader.make_error("packs do not have a sub type"));
        }

        let mut members = Vec::<_>::default();

        Self::parse_body(
            module,
            reader,
            |m, _, l| {
                let Some((mem_name, mem_type)) = l.split_once(":") else {
                    return Ok(());
                };

                let mem_name = mem_name.to_string();
                let mem_type = m.provision_type_id(mem_type)?;

                members.push((mem_name, mem_type));

                Ok(())
            },
            |_, r, _| Err(r.make_error("packs do not have a default")),
        )?;

        module.types.insert(tid, Type::Pack(Pack { members }));

        Ok(())
    }

    fn parse_enum(module: &mut Module, reader: &mut Reader) -> std::io::Result<()> {
        let (tid, sub_tid) = Module::parse_decl_head(module, reader)?;

        {
            let Some(Some(sub)) = sub_tid.map(|x| module.lookup(x)) else {
                return Err(reader.make_error("enums need a sub type"));
            };

            match sub {
                Type::Primitive(Primitive::F32) => {
                    return Err(reader.make_error("enums sub type must be integer"));
                }
                Type::Primitive(Primitive::F64) => {
                    return Err(reader.make_error("enums sub type must be integer"));
                }

                Type::Primitive(_) => {
                    // ok!
                }
                _ => {
                    return Err(reader.make_error("enums sub type must be integer"));
                }
            }
        }

        let mut members = Vec::<_>::default();

        Self::parse_body(module, reader, |m, l| {
            let Some((mem_name, mem_type)) = l.split_once("=") else {
                return Ok(());
            };

            let mem_name = mem_name.to_string();
            let mem_type: i64 = mem_type
                .parse()
                .map_err(|_| reader.make_error("enum values must be an integer"))?;

            members.push((mem_name, mem_type));

            Ok(())
        })?;

        module.types.insert(
            tid,
            Type::Enum(Enum {
                ty: (),
                members,
                default: (),
            }),
        );

        Ok(())
    }

    fn parse_bits(module: &mut Module, reader: &mut Reader) -> std::io::Result<()> {
        todo!()
    }

    fn parse_variant(module: &mut Module, reader: &mut Reader) -> std::io::Result<()> {
        todo!()
    }

    fn parse_seq(module: &mut Module, reader: &mut Reader) -> std::io::Result<()> {
        todo!()
    }
}

struct Reader {
    f: BufReader<std::fs::File>,
    current_line: String,
    current_line_num: usize,
}

impl Reader {
    fn new(path: &Path) -> std::io::Result<Self> {
        let f = std::fs::File::open(path)?;

        let f = std::io::BufReader::new(f);

        let current_line = String::with_capacity(1024);

        Ok(Self {
            f,
            current_line,
            current_line_num: 0,
        })
    }

    fn advance(&mut self) -> std::io::Result<Option<()>> {
        self.current_line.clear();
        let size = self.f.read_line(&mut self.current_line)?;

        dbg!(&self.current_line);

        if size == 0 {
            println!("EOF");
            return Ok(None);
        }

        let mut line: &str = self.current_line.as_str().trim();

        if !line.is_empty() {
            if let Some((x, _)) = self.current_line.split_once("//") {
                line = x;
            };
        }

        self.current_line_num += 1;

        Ok(Some(()))
    }

    fn current_line(&self) -> &str {
        &self.current_line
    }

    fn make_error<E: std::fmt::Display>(&self, context: E) -> std::io::Error {
        std::io::Error::other(format!(
            "At line {}: '{}'\n -> Error: {}",
            self.current_line_num,
            self.current_line.trim(),
            context
        ))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();

    let Some(x) = args.get(1) else {
        return Err(Box::new(std::io::Error::other("Missing input")));
    };

    let path = PathBuf::from(x);

    let module = Module::from_file(&path)?;

    dbg!(module);

    Ok(())
}
