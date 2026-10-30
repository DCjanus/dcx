use std::io::{Read, Write};

use anyhow::{Context, ensure};
use ssh_encoding::{Decode, DecodePem, Reader, pem::PemLabel};
use ssh_key::{Cipher, Kdf, PublicKey, public::KeyData};
use zeroize::Zeroizing;

use crate::AnyResult;

/// 提取 OpenSSH 私钥中包含的公钥；无需口令，不解析或验证私钥数据。
pub fn public_key<R: Read, W: Write>(mut reader: R, mut writer: W) -> AnyResult {
    let mut input = Zeroizing::new(Vec::new());
    reader
        .read_to_end(&mut input)
        .context("failed to read SSH private key")?;
    let public_key = OpenSshPublicKey::decode_pem(&*input)
        .context("invalid OpenSSH private key; expected BEGIN OPENSSH PRIVATE KEY format")?;
    let encoded = public_key
        .0
        .to_openssh()
        .context("failed to encode SSH public key")?;
    writeln!(writer, "{encoded}").context("failed to write SSH public key")?;
    writer.flush().context("failed to flush SSH public key")?;
    Ok(())
}

struct OpenSshPublicKey(PublicKey);

impl PemLabel for OpenSshPublicKey {
    const PEM_LABEL: &'static str = "OPENSSH PRIVATE KEY";
}

impl Decode for OpenSshPublicKey {
    type Error = anyhow::Error;

    fn decode(reader: &mut impl Reader) -> Result<Self, Self::Error> {
        let mut magic = [0; 15];
        Reader::read(reader, &mut magic)?;
        ensure!(&magic == b"openssh-key-v1\0", "invalid OpenSSH key magic");
        let cipher = Cipher::decode(reader)?;
        let kdf = Kdf::decode(reader)?;
        ensure!(
            cipher.is_none() == kdf.is_none(),
            "inconsistent cipher and KDF"
        );
        ensure!(u32::decode(reader)? == 1, "expected exactly one public key");
        let key = reader.read_prefixed(KeyData::decode)?;
        // 只检查私钥区的容器结构，避免完整私钥解析对 ECDSA 标量长度的额外限制。
        let private_data = Zeroizing::new(Vec::<u8>::decode(reader)?);
        ensure!(
            !private_data.is_empty() && private_data.len().is_multiple_of(cipher.block_size()),
            "invalid private key block length"
        );
        if cipher.has_tag() {
            Reader::read(reader, &mut [0; 16])?;
        }
        Ok(Self(key.into()))
    }
}
