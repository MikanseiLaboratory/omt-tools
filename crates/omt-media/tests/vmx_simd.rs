//! Verify bumped `vmx-rs` / `openmediatransport-rs` SIMD dispatch.

use omt_media::{VmxCodec, VmxConfig, VmxProfile, VmxSimdPath};

#[test]
fn vmx_simd_path_reports_and_encodes() {
    let enc = VmxCodec::new(VmxConfig {
        width: 1920,
        height: 1080,
        profile: VmxProfile::OmtHq,
        color_space: Default::default(),
    })
    .expect("create codec");

    let path = enc.simd_path();
    let caps = enc.simd_capabilities();
    eprintln!(
        "omt-media vmx path={path} caps={{ssse3:{},sse42:{},avx2:{},bmi2:{},neon:{}}}",
        caps.ssse3, caps.sse42, caps.avx2, caps.bmi2, caps.neon
    );

    assert!(matches!(
        path,
        VmxSimdPath::Scalar | VmxSimdPath::Sse128 | VmxSimdPath::Avx2 | VmxSimdPath::Neon
    ));
    assert_eq!(path.to_string(), path.as_str());
    assert_eq!(caps.select_path(960), path);

    #[cfg(target_arch = "x86_64")]
    {
        assert_ne!(path, VmxSimdPath::Neon);
        if caps.avx2 && caps.bmi2 {
            assert_eq!(path, VmxSimdPath::Avx2);
        } else if caps.sse42 && caps.ssse3 {
            assert_eq!(path, VmxSimdPath::Sse128);
        }
    }

    // UV width not multiple of 16 must not select AVX2.
    let odd = VmxCodec::new(VmxConfig::new(632, 64)).expect("odd width");
    assert_ne!(odd.simd_path(), VmxSimdPath::Avx2);

    let width = 256i32;
    let height = 144i32;
    let stride = (width as usize) * 2;
    let mut uyvy = vec![128u8; stride * height as usize];
    for y in 0..height as usize {
        for x in (0..width as usize).step_by(2) {
            let o = y * stride + x * 2;
            uyvy[o] = 100;
            uyvy[o + 1] = 16 + ((x + y) % 220) as u8;
            uyvy[o + 2] = 140;
            uyvy[o + 3] = 16 + ((x + 1 + y) % 220) as u8;
        }
    }

    let mut small = VmxCodec::new(VmxConfig {
        width,
        height,
        profile: VmxProfile::OmtLq,
        color_space: Default::default(),
    })
    .unwrap();
    let encode_path = small.simd_path();
    small.encode_uyvy(&uyvy, stride).unwrap();
    let mut bitstream = vec![0u8; 2 << 20];
    let len = small.save_to(&mut bitstream).unwrap();
    assert!(len > 16);

    let mut dec = VmxCodec::new(VmxConfig::new(width, height)).unwrap();
    assert_eq!(dec.simd_path(), encode_path);
    dec.load_from(&bitstream[..len]).unwrap();
    let mut out = vec![0u8; stride * height as usize];
    dec.decode_uyvy(&mut out, stride).unwrap();
    let mean: f32 = out.iter().map(|&b| b as f32).sum::<f32>() / out.len() as f32;
    assert!(mean > 1.0, "decoded empty on path {encode_path}");
}

#[test]
fn suite_core_path_label_aligns_with_vmx_capabilities() {
    use suite_core::SimdCapabilities;

    let host = SimdCapabilities::detect();
    let codec = VmxCodec::new(VmxConfig::new(1920, 1080)).expect("codec");
    let vmx_caps = codec.simd_capabilities();

    assert_eq!(host.ssse3, vmx_caps.ssse3);
    assert_eq!(host.sse42, vmx_caps.sse42);
    assert_eq!(host.avx2, vmx_caps.avx2);
    assert_eq!(host.bmi2, vmx_caps.bmi2);
    assert_eq!(host.neon, vmx_caps.neon);
    // Capability-preferred label (no UV gate) matches codec path for 1920-wide frames.
    assert_eq!(host.preferred_path_label(), codec.simd_path().as_str());
}
