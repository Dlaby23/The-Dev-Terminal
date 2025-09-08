use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty, Child};
use std::sync::{Arc, Mutex};
use std::io::Write;
use tokio::sync::mpsc;
use tracing::{info, error};

pub struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    _child: Box<dyn Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl PtyHandle {
    pub fn spawn(rows: u16, cols: u16) -> Result<(Self, mpsc::UnboundedReceiver<Vec<u8>>)> {
        let pty_system = native_pty_system();
        
        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        
        let pair = pty_system.openpty(pty_size)?;
        let mut cmd = CommandBuilder::new("/bin/zsh");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        
        let child = pair.slave.spawn_command(cmd)?;
        info!("Spawned zsh with PID: {:?}", child.process_id());
        
        let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let mut reader = pair.master.try_clone_reader()?;
        
        let (tx, rx) = mpsc::unbounded_channel();
        
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        info!("PTY EOF");
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if tx.send(data).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });
        
        Ok((
            Self {
                master: pair.master,
                _child: child,
                writer,
            },
            rx,
        ))
    }
    
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }
    
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}