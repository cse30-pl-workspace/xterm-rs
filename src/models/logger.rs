use std::sync::Arc;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use unsigned_varint::encode as varint;
use zstd::stream::write::Encoder as ZEncoder;

#[derive(Clone, Copy, Debug)]
enum EventKind {
    Input,
    Output,
    Resize,
}

#[derive(Debug)]
pub struct RawEvt {
    elapsed: f32,
    kind: EventKind,
    payload: Vec<u8>,
}
fn encode_payload(raw: Vec<u8>) -> (bool, Vec<u8>) {
    if raw.len() > 256 {
        let mut enc = ZEncoder::new(Vec::with_capacity(raw.len() / 2), 1).expect("create zstd encoder");
        enc.write_all(&raw).unwrap();
        let raw = enc.finish().unwrap();
        (true, raw)
    } else {
        (false, raw)
    }
}

fn write_cast(w: &mut BufWriter<std::fs::File>, e: RawEvt) -> std::io::Result<()> {
    w.write_all(&e.elapsed.to_le_bytes())?;

    let (compressed, data) = encode_payload(e.payload);
    let kind_byte = (e.kind as u8) | if compressed { 0b1000_0000 } else { 0 };
    w.write_all(&[kind_byte])?;

    let mut len_buf = [0u8; 5];
    let v_len = varint::u32(data.len() as u32, &mut len_buf);
    w.write_all(v_len)?;

    w.write_all(&data)?;
    w.flush()?;
    Ok(())
}

pub struct Logger {
    cast_tx: mpsc::UnboundedSender<RawEvt>,
    hb_tx: mpsc::UnboundedSender<u32>,
}

impl Logger {
    pub fn new(fn_cast: &str, fn_hb: &str) -> Arc<Self> {
        let cast_file = BufWriter::new(OpenOptions::new().create(true).append(true).open(fn_cast).unwrap());
        let hb_file = BufWriter::new(OpenOptions::new().create(true).append(true).open(fn_hb).unwrap());

        let (cast_tx, mut cast_rx) = mpsc::unbounded_channel::<RawEvt>();
        let (hb_tx, mut hb_rx) = mpsc::unbounded_channel::<u32>();

        tokio::spawn(async move {
            let mut cast_file = cast_file;
            let mut hb_file = hb_file;

            loop {
                tokio::select! {
                    Some(evt) = cast_rx.recv() => {
                        write_cast(&mut cast_file, evt).unwrap();
                    }
                    Some(ts)  = hb_rx.recv() => {
                        hb_file.write_all(&ts.to_le_bytes()).unwrap();
                        hb_file.flush().ok();
                    }
                    else => break,
                }
            }

            let _ = cast_file.flush();
            let _ = hb_file.flush();
        });

        Arc::new(Self { cast_tx, hb_tx })
    }

    pub fn input(&self, elapsed: f32, bytes: Vec<u8>) {
        self.cast_tx
            .send(RawEvt {
                elapsed,
                kind: EventKind::Input,
                payload: bytes,
            })
            .ok();
    }
    pub fn output(&self, elapsed: f32, bytes: Vec<u8>) {
        self.cast_tx
            .send(RawEvt {
                elapsed,
                kind: EventKind::Output,
                payload: bytes,
            })
            .ok();
    }
    pub fn resize(&self, elapsed: f32, rows: u16, cols: u16) {
        let mut p = Vec::with_capacity(4);
        p.extend_from_slice(&rows.to_le_bytes());
        p.extend_from_slice(&cols.to_le_bytes());
        self.cast_tx
            .send(RawEvt {
                elapsed,
                kind: EventKind::Resize,
                payload: p,
            })
            .ok();
    }
    pub fn heartbeat(&self) {
        let ts_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs() as u32;
        self.hb_tx.send(ts_sec).ok();
    }
}
