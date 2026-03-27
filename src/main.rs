use clap::*;
use ffmpeg_next::{self as ffmpeg};
use image::{Rgba, RgbaImage};
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};
use noise::*;
use rand::*;
use std::{error::Error, f64::consts::PI, fs::create_dir_all, io::stdin, path::*};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 2080, value_parser = value_parser!(u32).range(1..))]
    width: u32,

    #[arg(long, default_value_t = 2080, value_parser = value_parser!(u32).range(1..))]
    height: u32,

    #[arg(long, default_value_t = 0.01)]
    step_size: f64,

    #[arg(long, default_value_t = 100, value_parser = value_parser!(u32).range(1..))]
    steps: u32,

    #[arg(long, default_value_t = 1.0)]
    noise: f64,

    #[arg(long, default_value_t = 1.0, value_parser = positive)]
    blur: f32,

    #[arg(long, default_value = "0, 0, 0, 255")]
    bg_color: String,

    #[arg(long, default_value = "255, 255, 255, 255")]
    fg_color: String,

    #[arg(long, default_value_t = 5000)]
    particles: u32,

    #[arg(long, default_value_t = false)]
    line: bool,

    #[arg(long, default_value_t = false)]
    video: bool,

    #[arg(short, long, default_value = "flowfield")]
    output: String,

    #[arg(short, long)]
    seed: Option<u32>,
}

struct Particle {
    x: f64,
    y: f64,
    prev_x: f64,
    prev_y: f64,
}

impl Particle {
    fn new() -> Self {
        let x = random::<f64>();
        let y = random::<f64>();
        Self {
            x,
            y,
            prev_x: x,
            prev_y: y,
        }
    }

    fn step(&mut self, perlin: &Perlin, img: &mut RgbaImage, cfg: &RenderConfig) -> bool {
        let angle = (perlin.get([self.x * cfg.noise, self.y * cfg.noise]) + 2.0) * PI; // 1.0 or 2.0
        let aspect = cfg.width as f64 / cfg.height as f64;

        let dx = angle.cos() * cfg.step / aspect;
        let dy = angle.sin() * cfg.step;

        let new_x = self.x + dx;
        let new_y = self.y + dy;

        let wrapped = dx.abs() > 0.5 || dy.abs() > 0.5;

        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = wrap(new_x);
        self.y = wrap(new_y);

        if cfg.line && wrapped {
            return true;
        }

        if let Some((x1, y1, x2, y2)) = clipper(self.prev_x, self.prev_y, self.x, self.y) {
            draw(img, cfg, x1, y1, x2, y2);
            true
        } else {
            false
        }
    }
}

fn wrap(v: f64) -> f64 {
    ((v % 1.0) + 1.0) % 1.0
}

struct RenderConfig {
    width: u32,
    height: u32,
    step: f64,
    noise: f64,
    line: bool,
    fg: Rgba<u8>,
    bg: Rgba<u8>,
}

fn draw(img: &mut RgbaImage, cfg: &RenderConfig, x1: f64, y1: f64, x2: f64, y2: f64) {
    let (w, h) = (cfg.width as f64, cfg.height as f64);

    if cfg.line {
        let x1 = (x1 * w) as i32;
        let y1 = (y1 * h) as i32;
        let x2 = (x2 * w) as i32;
        let y2 = (y2 * h) as i32;

        if x1 >= 0
            && y1 >= 0
            && x1 < w as i32
            && y1 < h as i32
            && x2 >= 0
            && y2 >= 0
            && x2 < w as i32
            && y2 < h as i32
        {
            draw_antialiased_line_segment_mut(img, (x1, y1), (x2, y2), cfg.fg, interpolate);
        }
    } else {
        let px = (x1 * w) as i32;
        let py = (y1 * h) as i32;

        if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
            img.put_pixel(px as u32, py as u32, cfg.fg);
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(random);

    let render_w = (args.width as f64 * 1.25) as u32;
    let render_h = (args.height as f64 * 1.25) as u32;

    let fg = parse_rgba(&args.fg_color)?;
    let bg = parse_rgba(&args.bg_color)?;

    let output = resolve_output_path(&args)?;
    ensure_parent_exists(&output)?;

    let mut img = RgbaImage::from_pixel(render_w, render_h, bg);
    let perlin = Perlin::new(seed);

    let cfg = RenderConfig {
        width: render_w,
        height: render_h,
        step: args.step_size,
        noise: args.noise,
        line: args.line,
        fg,
        bg,
    };

    if args.video {
        render_video(&args, &output, &mut img, &perlin, &cfg)?;
    } else {
        render_image(&args, &output, &mut img, &perlin, &cfg)?;
    }

    println!("seed: {}, saved as {}", seed, output.display());
    Ok(())
}

fn render_image(
    args: &Args,
    path: &Path,
    img: &mut RgbaImage,
    perlin: &Perlin,
    cfg: &RenderConfig,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..args.particles {
        let mut p = Particle::new();
        for _ in 0..args.steps {
            p.step(perlin, img, cfg);
        }
    }

    let frame_img = {
        let cropped = crop_center(img, args.width, args.height);
        image::imageops::blur(&cropped, args.blur)
    };

    frame_img.save(path)?;

    Ok(())
}

fn render_video(
    args: &Args,
    path: &Path,
    img: &mut RgbaImage,
    perlin: &Perlin,
    cfg: &RenderConfig,
) -> Result<(), Box<dyn Error>> {
    ffmpeg::init()?;

    let mut output = ffmpeg::format::output(path.to_str().ok_or("Invalid output path")?)?;
    let codec =
        ffmpeg::codec::encoder::find(ffmpeg::codec::Id::H264).ok_or(ffmpeg::Error::InvalidData)?;

    let mut encoder = ffmpeg::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;

    encoder.set_width(args.width);
    encoder.set_height(args.height);
    encoder.set_format(ffmpeg::format::Pixel::YUV420P);
    encoder.set_frame_rate(Some(ffmpeg::Rational(60, 1)));
    encoder.set_time_base(ffmpeg::Rational(1, 60));
    encoder.set_max_b_frames(0);
    encoder.set_gop(0);

    if output
        .format()
        .flags()
        .contains(ffmpeg::format::flag::Flags::GLOBAL_HEADER)
    {
        encoder.set_flags(ffmpeg::codec::flag::Flags::GLOBAL_HEADER);
    }

    let mut open_encoder = encoder.open()?;
    let mut stream = output.add_stream(codec)?;
    stream.set_parameters(&open_encoder);
    output.write_header()?;

    let stream_index = stream.index();
    let stream_time_base = stream.time_base();

    let mut scaler = ffmpeg::software::scaling::context::Context::get(
        ffmpeg::format::Pixel::RGB24,
        args.width,
        args.height,
        ffmpeg::format::Pixel::YUV420P,
        args.width,
        args.height,
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    )?;

    let mut particles: Vec<_> = (0..args.particles).map(|_| Particle::new()).collect();

    let mut rgb_frame =
        ffmpeg::frame::Video::new(ffmpeg::format::Pixel::RGB24, args.width, args.height);
    let mut yuv_frame =
        ffmpeg::frame::Video::new(ffmpeg::format::Pixel::YUV420P, args.width, args.height);

    for pts in 0..args.steps {
        *img = RgbaImage::from_pixel(cfg.width, cfg.height, cfg.bg);
        for p in &mut particles {
            p.step(perlin, img, cfg);
        }

        let frame_img =
            image::imageops::blur(&crop_center(img, args.width, args.height), args.blur);

        rgba_to_rgb_frame(&frame_img, &mut rgb_frame);
        scaler.run(&rgb_frame, &mut yuv_frame)?;
        yuv_frame.set_pts(Some(pts as i64));

        open_encoder.send_frame(&yuv_frame)?;
        write_encoded_packets(
            &mut open_encoder,
            &mut output,
            60,
            stream_index,
            stream_time_base,
        );
    }

    open_encoder.send_eof()?;
    write_encoded_packets(
        &mut open_encoder,
        &mut output,
        60,
        stream_index,
        stream_time_base,
    );

    output.write_trailer()?;
    Ok(())
}

fn rgba_to_rgb_frame(src: &RgbaImage, dst: &mut ffmpeg::frame::Video) {
    let (w, h) = src.dimensions();
    let w = w as usize;
    let h = h as usize;

    let src = src.as_raw();
    let dst_stride = dst.stride(0) as usize;
    let dst_data = dst.data_mut(0);

    for y in 0..h {
        let src_row = &src[y * w * 4..(y + 1) * w * 4];
        let dst_row = &mut dst_data[y * dst_stride..y * dst_stride + w * 3];

        for x in 0..w {
            let s = x * 4;
            let d = x * 3;
            dst_row[d..d + 3].copy_from_slice(&src_row[s..s + 3]);
        }
    }
}

fn write_encoded_packets(
    encoder: &mut ffmpeg::codec::encoder::video::Encoder,
    output: &mut ffmpeg::format::context::Output,
    fps: u32,
    stream_index: usize,
    stream_base_time: ffmpeg::Rational,
) {
    let base_time = ffmpeg::Rational(1, fps as i32);
    let mut encoded = ffmpeg::Packet::empty();
    while encoder.receive_packet(&mut encoded).is_ok() {
        encoded.set_stream(stream_index);
        encoded.rescale_ts(base_time, stream_base_time);
        encoded.write_interleaved(output).unwrap();
    }
}

fn crop_center(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    image::imageops::crop_imm(img, (iw - w) / 2, (ih - h) / 2, w, h).to_image()
}

fn resolve_output_path(args: &Args) -> Result<PathBuf, Box<dyn Error>> {
    let path = Path::new(&args.output);

    let path = if path.extension().is_none() {
        if args.video {
            path.with_extension("mp4")
        } else {
            path.with_extension("png")
        }
    } else {
        path.to_path_buf()
    };

    Ok(find_free_path(&path))
}

fn ensure_parent_exists(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        if !parent.exists() && !parent.as_os_str().is_empty() {
            println!("Create directory {:?}? (y/n)", parent);
            loop {
                let mut input = String::new();
                stdin().read_line(&mut input)?;
                match input.trim() {
                    "y" => {
                        create_dir_all(parent)?;
                        break;
                    }
                    "n" => return Err("Directory does not exist".into()),
                    _ => println!("Please enter y/n"),
                }
            }
        }
    }
    Ok(())
}

fn parse_rgba(s: &str) -> Result<Rgba<u8>, String> {
    let vals: Vec<u8> = s
        .split(',')
        .map(|v| v.trim().parse().map_err(|_| "Invalid color".to_string()))
        .collect::<Result<_, _>>()?;

    match vals.as_slice() {
        [r, g, b, a] => Ok(Rgba([*r, *g, *b, *a])),
        _ => Err("Expected 4 values (R,G,B,A)".into()),
    }
}

fn positive(s: &str) -> Result<f32, String> {
    let v: f32 = s.parse().map_err(|_| "Invalid number".to_string())?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err("Must be > 0".into())
    }
}

fn clipper(px: f64, py: f64, x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    Some((px, py, x, y))
}

fn find_free_path(path: &Path) -> PathBuf {
    let mut i = 0;
    let mut p = path.to_path_buf();

    while p.exists() {
        p = path.with_file_name(format!(
            "{}_{}.{}",
            path.file_stem().unwrap().to_string_lossy(),
            i,
            path.extension().unwrap().to_string_lossy()
        ));
        i += 1;
    }
    p
}
