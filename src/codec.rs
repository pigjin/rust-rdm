use crate::model::{Decoder, contains_binary, display_bytes};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const FORMATS: &[&str] = &[
    "Raw",
    "JSON",
    "Unicode JSON",
    "YAML",
    "XML",
    "Hex",
    "Binary",
    "BitSet",
];
pub const DECODERS: &[&str] = &[
    "None", "Base64", "GZip", "Deflate", "ZStd", "LZ4", "Brotli", "Msgpack", "PHP", "Pickle",
];
#[derive(Clone, Debug)]
pub struct Converted {
    pub text: String,
    pub decode: String,
    pub format: String,
}

pub fn convert(bytes: &[u8], decode: &str, format: &str, custom: &[Decoder]) -> Result<Converted> {
    if bytes.is_empty() {
        return Ok(Converted {
            text: String::new(),
            decode: if decode.is_empty() || decode == "Auto" {
                "None".into()
            } else {
                decode.into()
            },
            format: if format.is_empty() || format == "Auto" {
                "Raw".into()
            } else {
                format.into()
            },
        });
    }
    let (data, decode) = if decode == "Auto" || decode.is_empty() {
        let numeric = bytes.iter().all(u8::is_ascii_digit);
        let mut detected = None;
        if bytes.len() >= 12
            && bytes.len() % 4 == 0
            && !bytes.iter().all(u8::is_ascii_digit)
            && !bytes.iter().all(|b| *b == bytes[0])
        {
            if let Ok(b) = STANDARD.decode(bytes)
                && !contains_binary(&b)
            {
                detected = Some((b, "Base64".into()));
            }
        }
        if detected.is_none() && !numeric {
            for d in ["GZip", "ZStd", "LZ4", "Msgpack", "PHP", "Pickle"] {
                let plausible = match d {
                    "GZip" => bytes.starts_with(&[0x1f, 0x8b]),
                    "ZStd" => bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]),
                    "LZ4" => bytes.starts_with(&[4, 34, 77, 24]),
                    "Msgpack" => bytes
                        .first()
                        .is_some_and(|b| (0x80..=0x9f).contains(b) || (0xdc..=0xdf).contains(b)),
                    "Pickle" => {
                        bytes.starts_with(&[0x80])
                            || (bytes.last() == Some(&b'.')
                                && bytes.first().is_some_and(|b| b"(]}INSVFdlc".contains(b)))
                    }
                    "PHP" => {
                        bytes.starts_with(b"a:")
                            || bytes.starts_with(b"O:")
                            || bytes.starts_with(b"s:")
                            || bytes.starts_with(b"i:")
                            || bytes.starts_with(b"d:")
                            || bytes.starts_with(b"b:1;")
                            || bytes == b"N;"
                    }
                    _ => false,
                };
                if plausible && let Ok(b) = decode_bytes(bytes, d, custom) {
                    detected = Some((b, d.into()));
                    break;
                }
            }
        }
        if detected.is_none() && !numeric {
            for d in custom.iter().filter(|d| d.auto) {
                if let Ok(b) = custom_command(bytes, &d.decode_path, &d.decode_args) {
                    detected = Some((b, d.name.clone()));
                    break;
                }
            }
        }
        detected.unwrap_or_else(|| (bytes.to_vec(), "None".into()))
    } else {
        let decoded = decode_bytes(bytes, decode, custom).unwrap_or_else(|_| bytes.to_vec());
        (
            if decode == "Base64" && contains_binary(&decoded) {
                bytes.to_vec()
            } else {
                decoded
            },
            decode.into(),
        )
    };
    let format = if format == "Auto" || format.is_empty() {
        let text = std::str::from_utf8(&data).unwrap_or_default().trim();
        if (text.starts_with('{') && text.ends_with('}'))
            || (text.starts_with('[') && text.ends_with(']'))
        {
            "JSON"
        } else if serde_yaml::from_slice::<serde_yaml::Value>(&data)
            .is_ok_and(|v| v.is_mapping() || v.is_null())
        {
            "YAML"
        } else if std::str::from_utf8(&data).is_ok_and(|text| {
            let text = text.trim();
            (text.starts_with('<') || text.ends_with('>')) && format_bytes(&data, "XML").is_ok()
        }) {
            "XML"
        } else if contains_binary(&data) {
            "Hex"
        } else {
            "Raw"
        }
    } else {
        format
    };
    let text = format_bytes(&data, format).unwrap_or_else(|_| display_bytes(&data));
    Ok(Converted {
        text,
        decode,
        format: format.into(),
    })
}
pub fn decode_bytes(bytes: &[u8], decode: &str, custom: &[Decoder]) -> Result<Vec<u8>> {
    match decode {
        "None" | "" => Ok(bytes.to_vec()),
        "Base64" => Ok(STANDARD.decode(bytes)?),
        "GZip" => read_all(flate2::read::GzDecoder::new(bytes)),
        "Deflate" => read_all(flate2::read::DeflateDecoder::new(bytes)),
        "ZStd" => Ok(zstd::stream::decode_all(bytes)?),
        "LZ4" => read_all(lz4_flex::frame::FrameDecoder::new(bytes)),
        "Brotli" => read_all(brotli::Decompressor::new(bytes, 4096)),
        "Msgpack" => {
            let v: serde_json::Value = rmp_serde::from_slice(bytes)?;
            json_to_bytes(&v)
        }
        "Pickle" => {
            match serde_pickle::from_slice::<serde_json::Value>(bytes, Default::default()) {
                Ok(v) => json_to_bytes(&v),
                Err(_) => pickle_command(bytes, false),
            }
        }
        "PHP" => php(bytes, false),
        name => {
            let d = custom
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| anyhow::anyhow!("转码器不存在：{name}"))?;
            custom_command(bytes, &d.decode_path, &d.decode_args)
        }
    }
}
fn json_to_bytes(v: &serde_json::Value) -> Result<Vec<u8>> {
    match v {
        serde_json::Value::String(s) => Ok(s.as_bytes().to_vec()),
        _ => Ok(serde_json::to_vec(v)?),
    }
}
fn read_all(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut out = vec![];
    reader
        .by_ref()
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut out)?;
    if out.len() > 512 * 1024 * 1024 {
        bail!("解压结果过大");
    }
    Ok(out)
}
fn format_json(text: &str, pretty: bool, unicode: bool) -> Result<String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' {
            let start = i;
            i += 1;
            let mut escape = false;
            while i < chars.len() {
                let c = chars[i];
                if !escape && c == '"' {
                    i += 1;
                    break;
                }
                if !escape && c == '\\' {
                    escape = true;
                } else {
                    escape = false;
                }
                i += 1;
            }
            let token = chars[start..i].iter().collect::<String>();
            if unicode {
                out.push_str(&serde_json::to_string(&serde_json::from_str::<String>(
                    &token,
                )?)?);
            } else {
                out.push_str(&token);
            }
            continue;
        }
        match c {
            '{' | '[' => {
                out.push(c);
                depth += 1;
                if pretty
                    && chars[i + 1..]
                        .iter()
                        .find(|c| !c.is_whitespace())
                        .is_some_and(|c| *c != '}' && *c != ']')
                {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                if pretty
                    && chars[..i]
                        .iter()
                        .rev()
                        .find(|c| !c.is_whitespace())
                        .is_some_and(|c| *c != '{' && *c != '[')
                {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
                out.push(c);
            }
            ',' => {
                out.push(c);
                if pretty
                    && chars[i + 1..]
                        .iter()
                        .find(|c| !c.is_whitespace())
                        .is_some_and(|c| *c != '}' && *c != ']')
                {
                    out.push('\n');
                    out.push_str(&"  ".repeat(depth));
                }
            }
            ':' => {
                out.push(c);
                if pretty {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    Ok(out)
}
pub fn format_bytes(bytes: &[u8], format: &str) -> Result<String> {
    Ok(match format {
        "Raw" => String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| display_bytes(bytes)),
        "JSON" => format_json(std::str::from_utf8(bytes)?, true, false)?,
        "Unicode JSON" => format_json(std::str::from_utf8(bytes)?, true, true)?,
        "YAML" => {
            let _: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
            display_bytes(bytes)
        }
        "XML" => {
            let mut reader = quick_xml::Reader::from_reader(bytes);
            let mut depth = 0usize;
            let mut has_element = false;
            loop {
                match reader.read_event()? {
                    quick_xml::events::Event::Start(_) => {
                        depth += 1;
                        has_element = true;
                    }
                    quick_xml::events::Event::Empty(_) => has_element = true,
                    quick_xml::events::Event::End(_) => {
                        depth = depth.checked_sub(1).ok_or_else(|| {
                            anyhow::anyhow!("XML closing tag without opening tag")
                        })?;
                    }
                    quick_xml::events::Event::Eof => {
                        if depth != 0 || !has_element {
                            bail!("incomplete XML document");
                        }
                        break;
                    }
                    _ => {}
                }
            }
            display_bytes(bytes)
        }
        "Hex" => bytes.iter().map(|b| format!("\\x{b:02X}")).collect(),
        "Binary" => bytes.iter().map(|b| format!("{b:08b}")).collect(),
        "BitSet" => bytes
            .iter()
            .enumerate()
            .flat_map(|(i, b)| {
                (0..8)
                    .filter(move |j| b & (1 << (7 - j)) != 0)
                    .map(move |j| (i * 8 + j).to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => bail!("未知显示格式：{format}"),
    })
}
pub fn encode(text: &str, decode: &str, format: &str, custom: &[Decoder]) -> Result<Vec<u8>> {
    let raw = match format {
        "Hex" => hex::decode(
            text.replace("\\x", "")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        )?,
        "Binary" => {
            let s = text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            if s.len() % 8 != 0 {
                bail!("二进制长度必须是 8 的倍数");
            }
            s.as_bytes()
                .chunks(8)
                .map(|b| Ok(u8::from_str_radix(std::str::from_utf8(b)?, 2)?))
                .collect::<Result<Vec<u8>>>()?
        }
        "BitSet" => {
            let positions = text
                .lines()
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().parse::<usize>())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let max = positions.iter().copied().max().unwrap_or(0);
            if max > u32::MAX as usize {
                bail!("位索引超过范围");
            }
            let mut bytes = vec![
                0;
                if positions.is_empty() {
                    0
                } else {
                    ((max + 7) / 8) + 1
                }
            ];
            for n in positions {
                bytes[n / 8] |= 1 << (7 - n % 8);
            }
            bytes
        }
        "JSON" | "Unicode JSON" => format_json(text, false, false)?.into_bytes(),
        "YAML" => text.as_bytes().to_vec(),
        "XML" => {
            format_bytes(text.as_bytes(), "XML")?;
            text.as_bytes().to_vec()
        }
        _ => text.as_bytes().to_vec(),
    };
    encode_bytes(raw, decode, custom)
}

fn encode_bytes(raw: Vec<u8>, decode: &str, custom: &[Decoder]) -> Result<Vec<u8>> {
    match decode {
        "None" | "" | "Auto" => Ok(raw),
        "Base64" => Ok(STANDARD.encode(raw).into_bytes()),
        "GZip" => {
            let mut w = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            w.write_all(&raw)?;
            Ok(w.finish()?)
        }
        "Deflate" => {
            let mut w =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            w.write_all(&raw)?;
            Ok(w.finish()?)
        }
        "ZStd" => Ok(zstd::stream::encode_all(raw.as_slice(), 0)?),
        "LZ4" => {
            let mut w = lz4_flex::frame::FrameEncoder::new(Vec::new());
            w.write_all(&raw)?;
            Ok(w.finish()?)
        }
        "Brotli" => {
            let mut out = vec![];
            {
                let mut w = brotli::CompressorWriter::new(&mut out, 4096, 5, 22);
                w.write_all(&raw)?;
            }
            Ok(out)
        }
        "Msgpack" => {
            let v = serde_json::from_slice::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::Value::String(display_bytes(&raw)));
            Ok(rmp_serde::to_vec_named(&v)?)
        }
        "Pickle" => {
            let v = serde_json::from_slice::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::Value::String(display_bytes(&raw)));
            let data = serde_json::to_vec(&v)?;
            match pickle_command(&data, true) {
                Ok(value) => Ok(value),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    Ok(serde_pickle::to_vec(&v, Default::default())?)
                }
                Err(error) => Err(error),
            }
        }
        "PHP" => php(&raw, true),
        name => {
            let d = custom
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| anyhow::anyhow!("转码器不存在：{name}"))?;
            custom_command(&raw, &d.encode_path, &d.encode_args)
        }
    }
}

/// Original SaveAs: validate/format first, then an encoder consumes the
/// original input (not the formatted result). Collection edits use this API.
pub fn save_as(bytes: &[u8], decode: &str, format: &str, custom: &[Decoder]) -> Result<Vec<u8>> {
    let invalid = || anyhow::anyhow!("invalid {format} data");
    let formatted = match format {
        "JSON" | "Unicode JSON" => {
            let text = source_string(bytes);
            let mut chars = text.chars();
            let mut result = String::new();
            while let Some(ch) = chars.next() {
                if ch == '"' {
                    result.push(ch);
                    let mut escaped = false;
                    for ch in chars.by_ref() {
                        if ch == '\n' { break; }
                        result.push(ch);
                        if escaped { escaped = false; }
                        else if ch == '\\' { escaped = true; }
                        else if ch == '"' { break; }
                    }
                } else if !ch.is_whitespace() {
                    result.push(ch);
                }
            }
            result.into_bytes()
        }
        "Hex" => hex::decode(source_string(bytes).replace("\\x", "")).map_err(|_| invalid())?,
        "Binary" => {
            if bytes.len() % 8 != 0 { return Err(invalid()); }
            bytes.chunks_exact(8).map(|chunk| {
                std::str::from_utf8(chunk).ok().and_then(|text|u8::from_str_radix(text,2).ok()).ok_or_else(invalid)
            }).collect::<Result<Vec<_>>>()?
        }
        "BitSet" => {
            let text=source_string(bytes).replace("\r\n","\n").replace('\r',"\n");
            let positions=text.split('\n').filter_map(|line|line.parse::<u32>().ok()).collect::<Vec<_>>();
            let mut result=vec![0;positions.iter().copied().max().map(|n|((n as usize+7)/8)+1).unwrap_or(0)];
            for n in positions { result[n as usize/8]|=1<<(7-n%8); }
            result
        }
        _ => bytes.to_vec(),
    };
    if (DECODERS.contains(&decode) && decode != "None") || custom.iter().any(|item| item.name == decode) {
        encode_bytes(bytes.to_vec(),decode,custom).map_err(|_| anyhow::anyhow!("fail to build {decode}"))
    } else {
        Ok(formatted)
    }
}

/// Go's JSON writer replaces each invalid UTF-8 byte with one replacement rune.
pub(crate) fn source_string(mut bytes: &[u8]) -> String {
    let mut text=String::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(valid)=>{text.push_str(valid);break;}
            Err(error)=>{
                text.push_str(std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap());
                let count=error.error_len().unwrap_or(bytes.len()-error.valid_up_to());
                text.extend(std::iter::repeat_n('\u{fffd}',count));
                bytes=&bytes[error.valid_up_to()+count..];
            }
        }
    }
    text
}
fn pickle_command(data: &[u8], encode: bool) -> Result<Vec<u8>> {
    // Match the original datetime conversion while refusing executable globals.
    let script = r#"import base64, json, pickle, sys, io, datetime, collections, builtins
def hook(obj):
    for k,v in obj.items():
        if isinstance(v,str):
            try: obj[k]=datetime.datetime.fromisoformat(v)
            except ValueError: pass
    return obj
def serialize(o):
    if isinstance(o,datetime.datetime): return o.isoformat()
    return str(o)
class Reader(pickle.Unpickler):
    def find_class(self,module,name):
        if module=='datetime' and name in ('datetime','date','time','timedelta','timezone'): return getattr(datetime,name)
        if module=='collections' and name=='OrderedDict': return collections.OrderedDict
        if module in ('builtins','__builtin__') and name in ('set','frozenset','bytes','bytearray','complex'): return getattr(builtins,name)
        if module=='_codecs' and name=='encode':
            return lambda value,encoding='latin1': value.encode(encoding) if encoding.lower() in ('latin1','latin-1','utf-8','ascii') else (_ for _ in ()).throw(ValueError('unsupported encoding'))
        raise ValueError('unsupported pickle global')
data=base64.b64decode(sys.argv[2])
if sys.argv[1]=='encode': result=pickle.dumps(json.loads(data,object_hook=hook),protocol=4)
else: result=json.dumps(Reader(io.BytesIO(data)).load(),ensure_ascii=False,default=serialize).encode('utf-8')
print(base64.b64encode(result).decode('ascii'))
"#;
    let args = vec![
        "-c".into(),
        script.into(),
        if encode {
            "encode".into()
        } else {
            "decode".into()
        },
    ];
    match custom_command(data, "python3", &args) {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            custom_command(data, "python", &args)
        }
        result => result,
    }
}
fn php(data: &[u8], encode: bool) -> Result<Vec<u8>> {
    let code = if encode {
        "$v=json_decode(base64_decode($argv[1]),true);echo base64_encode(serialize($v));"
    } else {
        "$v=unserialize(base64_decode($argv[1]));echo base64_encode(json_encode($v,JSON_UNESCAPED_UNICODE|JSON_UNESCAPED_SLASHES));"
    };
    custom_command(data, "php", &["-r".into(), code.into()])
}
pub fn custom_command(data: &[u8], path: &str, args: &[String]) -> Result<Vec<u8>> {
    let value = STANDARD.encode(data);
    let has = args.iter().any(|a| a.contains("{VALUE}"));
    let mut args = args
        .iter()
        .map(|a| a.trim().replace("{VALUE}", &value))
        .collect::<Vec<_>>();
    if !has {
        args.push(value);
    }
    let mut child = Command::new(path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || read_all(stdout));
    let err = std::thread::spawn(move || read_all(stderr));
    let begin = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if begin.elapsed() > Duration::from_secs(30) {
            child.kill()?;
            child.wait()?;
            bail!("自定义转码器执行超时");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let out = out
        .join()
        .map_err(|_| anyhow::anyhow!("读取转码输出失败"))??;
    let err = err
        .join()
        .map_err(|_| anyhow::anyhow!("读取转码错误失败"))??;
    if !status.success() || out.starts_with(b"[RDM-ERROR]") {
        bail!("转码器执行失败：{}", String::from_utf8_lossy(&err));
    }
    let out = String::from_utf8(out)?;
    Ok(STANDARD.decode(out.trim())?)
}
