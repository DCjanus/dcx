use std::io::{Read, Write};

use anyhow::Context;
use ssh_key::PrivateKey;
use zeroize::Zeroizing;

use crate::AnyResult;

/// 提取 OpenSSH 私钥中包含的公钥；加密私钥也无需口令，不验证其加密部分。
pub fn public_key<R: Read, W: Write>(mut reader: R, mut writer: W) -> AnyResult {
    let mut input = Zeroizing::new(Vec::new());
    reader
        .read_to_end(&mut input)
        .context("failed to read SSH private key")?;
    let private_key = PrivateKey::from_openssh(&*input)
        .context("invalid OpenSSH private key; expected BEGIN OPENSSH PRIVATE KEY format")?;
    // 注释可能位于加密部分，统一省略以保持输出格式一致。
    let mut public_key = private_key.public_key().clone();
    public_key.set_comment("");
    let encoded = public_key
        .to_openssh()
        .context("failed to encode SSH public key")?;
    writeln!(writer, "{encoded}").context("failed to write SSH public key")?;
    writer.flush().context("failed to flush SSH public key")?;
    Ok(())
}
