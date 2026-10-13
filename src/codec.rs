use crate::model::{Decoder, display_bytes};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{io::{Read, Write}, process::{Command, Stdio}, time::{Duration,Instant}};

pub const FORMATS:&[&str]=&["Raw","JSON","Unicode JSON","YAML","XML","Hex","Binary","BitSet"];
pub const DECODERS:&[&str]=&["None","Base64","GZip","Deflate","ZStd","LZ4","Brotli","Msgpack","PHP","Pickle"];
#[derive(Clone,Debug)]
pub struct Converted { pub text:String, pub decode:String, pub format:String }

pub fn convert(bytes:&[u8],decode:&str,format:&str,custom:&[Decoder]) -> Result<Converted> {
    let (data,decode)=if decode=="Auto" || decode.is_empty() {
        let mut detected=None;
        if bytes.len()>=12 && bytes.len()%4==0 && !bytes.iter().all(u8::is_ascii_digit) && !bytes.iter().all(|b|*b==bytes[0]) {
            if let Ok(b)=STANDARD.decode(bytes) && std::str::from_utf8(&b).is_ok() { detected=Some((b,"Base64".into())); }
        }
        if detected.is_none() {
            for d in ["GZip","ZStd","LZ4","Brotli","Msgpack"] {
                let plausible=match d { "GZip"=>bytes.starts_with(&[0x1f,0x8b]),"ZStd"=>bytes.starts_with(&[0x28,0xb5,0x2f,0xfd]),"LZ4"=>bytes.starts_with(&[4,34,77,24]),"Msgpack"=>bytes.first().is_some_and(|b|(0x80..=0x9f).contains(b)||(0xdc..=0xdf).contains(b)),_=>false };
                if plausible && let Ok(b)=decode_bytes(bytes,d,custom) {detected=Some((b,d.into()));break;}
            }
        }
        if detected.is_none() { for d in custom.iter().filter(|d|d.enable&&d.auto) { if let Ok(b)=custom_command(bytes,&d.decode_path,&d.decode_args) { detected=Some((b,d.name.clone()));break; } } }
        detected.unwrap_or_else(||(bytes.to_vec(),"None".into()))
    } else { (decode_bytes(bytes,decode,custom)?,decode.into()) };
    let format=if format=="Auto" || format.is_empty() {
        if serde_json::from_slice::<serde_json::Value>(&data).is_ok() { "JSON" }
        else if data.starts_with(b"<") { "XML" }
        else if std::str::from_utf8(&data).is_err() { "Hex" }
        else { "Raw" }
    } else { format };
    let text=format_bytes(&data,format)?;
    Ok(Converted{text,decode,format:format.into()})
}
pub fn decode_bytes(bytes:&[u8],decode:&str,custom:&[Decoder]) -> Result<Vec<u8>> {
    match decode {
        "None"|""=>Ok(bytes.to_vec()),"Base64"=>Ok(STANDARD.decode(bytes)?),
        "GZip"=>read_all(flate2::read::GzDecoder::new(bytes)),
        "Deflate"=>read_all(flate2::read::DeflateDecoder::new(bytes)),
        "ZStd"=>Ok(zstd::stream::decode_all(bytes)?),
        "LZ4"=>read_all(lz4_flex::frame::FrameDecoder::new(bytes)),
        "Brotli"=>read_all(brotli::Decompressor::new(bytes,4096)),
        "Msgpack"=>{ let v:serde_json::Value=rmp_serde::from_slice(bytes)?; json_to_bytes(&v) },
        "Pickle"=>{ let v:serde_json::Value=serde_pickle::from_slice(bytes,Default::default())?; json_to_bytes(&v) },
        "PHP"=>php(bytes,false),
        name=>{ let d=custom.iter().find(|d|d.enable&&d.name==name).ok_or_else(||anyhow::anyhow!("转码器不存在：{name}"))?; custom_command(bytes,&d.decode_path,&d.decode_args) }
    }
}
fn json_to_bytes(v:&serde_json::Value) -> Result<Vec<u8>> { match v { serde_json::Value::String(s)=>Ok(s.as_bytes().to_vec()),_=>Ok(serde_json::to_vec(v)?) } }
fn read_all(mut reader:impl Read) -> Result<Vec<u8>> { let mut out=vec![]; reader.by_ref().take(512*1024*1024+1).read_to_end(&mut out)?; if out.len()>512*1024*1024 {bail!("解压结果过大");} Ok(out) }
pub fn format_bytes(bytes:&[u8],format:&str) -> Result<String> {
    Ok(match format {
        "Raw"=>display_bytes(bytes),
        "JSON"|"Unicode JSON"=>serde_json::to_string_pretty(&serde_json::from_slice::<serde_json::Value>(bytes)?)?,
        "YAML"=>{ let v:serde_yaml::Value=serde_yaml::from_slice(bytes)?; serde_yaml::to_string(&v)? },
        "XML"=>{
            let mut reader=quick_xml::Reader::from_reader(bytes); reader.config_mut().trim_text(true);
            let mut w=quick_xml::Writer::new_with_indent(Vec::new(),b' ',2);
            loop { let e=reader.read_event()?; if matches!(e,quick_xml::events::Event::Eof) {break;} w.write_event(e)?; }
            String::from_utf8(w.into_inner())?
        },
        "Hex"=>bytes.iter().map(|b|format!("\\x{b:02X}")).collect(),
        "Binary"=>bytes.iter().map(|b|format!("{b:08b}")).collect(),
        "BitSet"=>bytes.iter().enumerate().flat_map(|(i,b)|(0..8).filter(move |j|b&(1<<(7-j))!=0).map(move |j|(i*8+j).to_string())).collect::<Vec<_>>().join("\n"),
        _=>bail!("未知显示格式：{format}"),
    })
}
pub fn encode(text:&str,decode:&str,format:&str,custom:&[Decoder]) -> Result<Vec<u8>> {
    let raw=match format {
        "Hex"=>hex::decode(text.replace("\\x", "").chars().filter(|c|!c.is_whitespace()).collect::<String>())?,
        "Binary"=>{ let s=text.chars().filter(|c|!c.is_whitespace()).collect::<String>(); if s.len()%8!=0 {bail!("二进制长度必须是 8 的倍数");} s.as_bytes().chunks(8).map(|b|Ok(u8::from_str_radix(std::str::from_utf8(b)?,2)?)).collect::<Result<Vec<u8>>>()? },
        "BitSet"=>{
            let positions=text.lines().filter(|s|!s.trim().is_empty()).map(|s|s.trim().parse::<usize>()).collect::<std::result::Result<Vec<_>,_>>()?;
            let max=positions.iter().copied().max().unwrap_or(0); if max>u32::MAX as usize {bail!("位索引超过范围");}
            let mut bytes=vec![0; if positions.is_empty(){0}else{((max+7)/8)+1}]; for n in positions {bytes[n/8]|=1<<(7-n%8);} bytes
        },
        "JSON"|"Unicode JSON"=>serde_json::to_vec(&serde_json::from_str::<serde_json::Value>(text)?)?,
        "YAML"=>serde_yaml::to_string(&serde_yaml::from_str::<serde_yaml::Value>(text)?)?.into_bytes(),
        "XML"=>{ format_bytes(text.as_bytes(),"XML")?;text.as_bytes().to_vec() },
        _=>text.as_bytes().to_vec(),
    };
    match decode {
        "None"|""|"Auto"=>Ok(raw),"Base64"=>Ok(STANDARD.encode(raw).into_bytes()),
        "GZip"=>{ let mut w=flate2::write::GzEncoder::new(Vec::new(),flate2::Compression::default());w.write_all(&raw)?;Ok(w.finish()?) },
        "Deflate"=>{let mut w=flate2::write::DeflateEncoder::new(Vec::new(),flate2::Compression::default());w.write_all(&raw)?;Ok(w.finish()?)},
        "ZStd"=>Ok(zstd::stream::encode_all(raw.as_slice(),0)?),
        "LZ4"=>{let mut w=lz4_flex::frame::FrameEncoder::new(Vec::new());w.write_all(&raw)?;Ok(w.finish()?)},
        "Brotli"=>{let mut out=vec![];{let mut w=brotli::CompressorWriter::new(&mut out,4096,5,22);w.write_all(&raw)?;}Ok(out)},
        "Msgpack"=>{let v=serde_json::from_slice::<serde_json::Value>(&raw).unwrap_or_else(|_|serde_json::Value::String(display_bytes(&raw)));Ok(rmp_serde::to_vec_named(&v)?)},
        "Pickle"=>{let v=serde_json::from_slice::<serde_json::Value>(&raw).unwrap_or_else(|_|serde_json::Value::String(display_bytes(&raw)));Ok(serde_pickle::to_vec(&v,Default::default())?)},
        "PHP"=>php(&raw,true),
        name=>{let d=custom.iter().find(|d|d.enable&&d.name==name).ok_or_else(||anyhow::anyhow!("转码器不存在：{name}"))?;custom_command(&raw,&d.encode_path,&d.encode_args)}
    }
}
fn php(data:&[u8],encode:bool) -> Result<Vec<u8>> {
    let code=if encode { "$v=json_decode(base64_decode($argv[1]),true);echo base64_encode(serialize($v));" } else { "$v=unserialize(base64_decode($argv[1]));echo base64_encode(json_encode($v,JSON_UNESCAPED_UNICODE|JSON_UNESCAPED_SLASHES));" };
    custom_command(data,"php",&["-r".into(),code.into()])
}
pub fn custom_command(data:&[u8],path:&str,args:&[String]) -> Result<Vec<u8>> {
    let value=STANDARD.encode(data); let has=args.iter().any(|a|a.contains("{VALUE}"));
    let mut args=args.iter().map(|a|a.trim().replace("{VALUE}",&value)).collect::<Vec<_>>(); if !has {args.push(value);}
    let mut child=Command::new(path).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout=child.stdout.take().unwrap(); let stderr=child.stderr.take().unwrap();
    let out=std::thread::spawn(move||read_all(stdout)); let err=std::thread::spawn(move||read_all(stderr));
    let begin=Instant::now();
    let status=loop { if let Some(status)=child.try_wait()? {break status;} if begin.elapsed()>Duration::from_secs(30) {child.kill()?;child.wait()?;bail!("自定义转码器执行超时");} std::thread::sleep(Duration::from_millis(10)); };
    let out=out.join().map_err(|_|anyhow::anyhow!("读取转码输出失败"))??; let err=err.join().map_err(|_|anyhow::anyhow!("读取转码错误失败"))??;
    if !status.success() || out.starts_with(b"[RDM-ERROR]") {bail!("转码器执行失败：{}",String::from_utf8_lossy(&err));}
    let out=String::from_utf8(out)?;Ok(STANDARD.decode(out.trim())?)
}
