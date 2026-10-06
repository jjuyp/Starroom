//! Controlled CFA/DNG diagnostic using the existing TIFF codec and real LibRaw sensor decoder.
//! All samples are authored synthetic sensor data, not a camera quality/photographic fixture.
use std::{
    error::Error,
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tiff::{
    encoder::{Rational, SRational, TiffEncoder, colortype::Gray16},
    tags::Tag,
};

pub struct OwnedFixture(pub PathBuf);
impl Drop for OwnedFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn controlled_fixture() -> Result<OwnedFixture, Box<dyn Error>> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut bytes)?;
        let mut image = encoder.new_image::<Gray16>(64, 64)?;
        let dir = image.encoder();
        dir.write_tag(Tag::PhotometricInterpretation, 32803u16)?;
        dir.write_tag(Tag::Make, "Starroom")?;
        dir.write_tag(Tag::Model, "Controlled Bayer")?;
        dir.write_tag(Tag::Unknown(50706), &[1u8, 4, 0, 0][..])?;
        dir.write_tag(Tag::Unknown(50707), &[1u8, 1, 0, 0][..])?;
        dir.write_tag(Tag::Unknown(50708), "Starroom Controlled Bayer")?;
        dir.write_tag(Tag::Unknown(33421), &[2u16, 2][..])?;
        dir.write_tag(Tag::Unknown(33422), &[0u8, 1, 1, 2][..])?;
        dir.write_tag(Tag::Unknown(50710), &[0u8, 1, 2][..])?;
        dir.write_tag(Tag::Unknown(50711), 1u16)?;
        dir.write_tag(Tag::Unknown(50713), &[1u16, 1][..])?;
        dir.write_tag(Tag::Unknown(50714), Rational { n: 0, d: 1 })?;
        dir.write_tag(Tag::Unknown(50717), 16383u32)?;
        dir.write_tag(Tag::Unknown(50719), &[0u32, 0][..])?;
        dir.write_tag(Tag::Unknown(50720), &[64u32, 64][..])?;
        let matrix: Vec<_> = [1, 0, 0, 0, 1, 0, 0, 0, 1]
            .into_iter()
            .map(|n| SRational { n, d: 1 })
            .collect();
        dir.write_tag(Tag::Unknown(50721), matrix.as_slice())?;
        dir.write_tag(
            Tag::Unknown(50728),
            &[
                Rational { n: 1, d: 2 },
                Rational { n: 1, d: 1 },
                Rational { n: 1, d: 4 },
            ][..],
        )?;
        dir.write_tag(Tag::Unknown(50778), 21u16)?;
        dir.write_tag(Tag::Unknown(50829), &[0u32, 0, 64, 64][..])?;
        let levels = [1024u16, 2048, 4096, 6144, 8192, 10240, 12288, 14336];
        let sensor: Vec<_> = (0..4096).map(|index| levels[(index % 64) / 8]).collect();
        image.write_data(&sensor)?;
    }
    let path = std::env::temp_dir().join(format!(
        "starroom-owned-headroom-{}-{}.dng",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let mut file = fs::File::create_new(&path)?;
    let fixture = OwnedFixture(path);
    file.write_all(bytes.get_ref())?;
    file.sync_all()?;
    drop(file);
    Ok(fixture)
}
