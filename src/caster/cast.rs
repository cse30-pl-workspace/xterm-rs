use base64::Engine as _;
use std::sync::Arc;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::mpsc,
    time::{self, Duration},
};
use unsigned_varint::encode as varint;
use zstd::stream::encode_all;
use crate::models::logger;

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

fn encode_evt(e: &RawEvt) -> Vec<u8> {
    // estimate 4(elapsed)+1(kind)+5(varint)+payload
    let mut v = Vec::with_capacity(10 + e.payload.len());
    v.extend_from_slice(&e.elapsed.to_le_bytes());
    v.push(e.kind as u8);

    let mut len_buf = [0u8; 5];
    let var = varint::u32(e.payload.len() as u32, &mut len_buf);
    v.extend_from_slice(var);
    v.extend_from_slice(&e.payload);
    v
}

fn write_cast(file: &mut BufWriter<std::fs::File>, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.flush()
}

pub struct Caster {
    cast_tx: mpsc::UnboundedSender<RawEvt>,
    hb_tx: mpsc::UnboundedSender<u32>,
}

impl Caster {
    pub fn new(
        log_dir: std::path::PathBuf,
        timestamp: u128,
        fn_cast: &str,
        fn_hb: &str,
        verbose_log: bool,
        verbose_interval: u32,
    ) -> anyhow::Result<Arc<Self>> {
        if log_dir.exists() && !log_dir.is_dir() {
            anyhow::bail!("'{}' exists and is not a directory", log_dir.display());
        }
        std::fs::create_dir_all(&log_dir)?;

        let cast_path = log_dir.join(fn_cast);
        let hb_path = log_dir.join(fn_hb);

        let cast_file = BufWriter::new(OpenOptions::new().create(true).append(true).open(&cast_path)?);
        let hb_file = BufWriter::new(OpenOptions::new().create(true).append(true).open(&hb_path)?);

        let (cast_tx, mut cast_rx) = mpsc::unbounded_channel::<RawEvt>();
        let (hb_tx, mut hb_rx) = mpsc::unbounded_channel::<u32>();

        tokio::spawn(async move {
            let mut cast_file = cast_file;
            let mut hb_file = hb_file;
            let mut verbose_buf: Vec<u8> = Vec::new();
            let mut tick = time::interval(Duration::from_secs(verbose_interval.into()));
            tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    Some(evt) = cast_rx.recv() => {
                        let bytes = encode_evt(&evt);
                        write_cast(&mut cast_file, &bytes).unwrap();
                        if verbose_log  {
                            verbose_buf.extend_from_slice(&bytes);
                        }
                    }
                    Some(ts)  = hb_rx.recv() => {
                        hb_file.write_all(&ts.to_le_bytes()).unwrap();
                        hb_file.flush().ok();
                    }
                    _ = tick.tick(), if verbose_log => {
                        if !verbose_buf.is_empty() {
                            let cmp = encode_all(&verbose_buf[..], 3);
                            let b64 = match cmp {
                                Ok(cmp) => base64::engine::general_purpose::STANDARD.encode(&cmp),
                                Err(_) => "ErrorParsingString".to_string(),
                            };
                            let payload = serde_json::json!([timestamp, b64]);
                            logger("cast", payload);
                            verbose_buf.clear();
                        }
                    }
                    else => break,

                }
            }

            let _ = cast_file.flush();
            let _ = hb_file.flush();
        });

        Ok(Arc::new(Self { cast_tx, hb_tx }))
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
