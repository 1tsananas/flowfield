use clap::*;
use image::{Rgba, RgbaImage};
use imageproc::{drawing::draw_antialiased_line_segment_mut, pixelops::interpolate};
use ndarray::Array3;
use noise::*;
use rand::*;
use std::{error::Error, f64::consts::PI, fs::create_dir_all, io::stdin, path::*};
use video_rs::encode::{Encoder, Settings};
use video_rs::time::Time;

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
        let angle = (perlin.get([self.x * cfg.noise * cfg.aspect, self.y * cfg.noise]) + 1.0) * PI;

        let new_x = self.x + angle.cos() * cfg.step / cfg.aspect;
        let new_y = self.y + angle.sin() * cfg.step;

        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = new_x;
        self.y = new_y;

        if let Some((x1, y1, x2, y2)) = clipper(self.prev_x, self.prev_y, self.x, self.y) {
            draw(img, cfg, x1, y1, x2, y2);
            true
        } else {
            false
        }
    }
}

struct RenderConfig {
    width: u32,
    height: u32,
    step: f64,
    noise: f64,
    aspect: f64,
    line: bool,
    fg: Rgba<u8>,
}

fn draw(img: &mut RgbaImage, cfg: &RenderConfig, x1: f64, y1: f64, x2: f64, y2: f64) {
    let (w, h) = (cfg.width as f64, cfg.height as f64);

    if cfg.line {
        draw_antialiased_line_segment_mut(
            img,
            ((x1 * w) as i32, (y1 * h) as i32),
            ((x2 * w) as i32, (y2 * h) as i32),
            cfg.fg,
            interpolate,
        );
    } else {
        img.put_pixel((x1 * w) as u32, (y1 * h) as u32, cfg.fg);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(random);

    let bg = parse_rgba(&args.bg_color)?;
    let fg = parse_rgba(&args.fg_color)?;

    let output = resolve_output_path(&args)?;
    ensure_parent_exists(&output)?;

    let render_w = (args.width as f64 * 1.25) as u32;
    let render_h = (args.height as f64 * 1.25) as u32;
    let aspect = render_w as f64 / render_h as f64;

    let mut img = RgbaImage::from_pixel(render_w, render_h, bg);
    let perlin = Perlin::new(seed);

    let cfg = RenderConfig {
        width: render_w,
        height: render_h,
        step: args.step_size,
        noise: args.noise,
        aspect,
        line: args.line,
        fg,
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
            if !p.step(perlin, img, cfg) {
                break;
            }
        }
    }

    let img = crop_center(img, args.width, args.height);
    let img = image::imageops::blur(&img, args.blur);
    img.save(path)?;

    Ok(())
}

fn render_video(
    args: &Args,
    path: &Path,
    img: &mut RgbaImage,
    perlin: &Perlin,
    cfg: &RenderConfig,
) -> Result<(), Box<dyn Error>> {
    video_rs::init()?;

    let w = args.width & !15;
    let h = args.height & !15;

    let settings = Settings::preset_h264_yuv420p(w as usize, h as usize, false);
    println!("{}", path.to_string_lossy());
    let mut encoder = Encoder::new(path, settings)?;
    let mut particles: Vec<_> = (0..args.particles).map(|_| Particle::new()).collect();

    let mut time = Time::zero();
    let frame_time = Time::from_nth_of_a_second(24);

    for _ in 0..args.steps {
        for p in &mut particles {
            p.step(perlin, img, cfg);
        }

        let frame = prepare_frame_yuv(img, w, h, args.blur);
        eprintln!("3) before encode");
        encoder.encode(&frame, time)?;
        time = time.aligned_with(frame_time).add();
    }

    encoder.finish()?;
    Ok(())
}

fn crop_center(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    image::imageops::crop_imm(img, (iw - w) / 2, (ih - h) / 2, w, h).to_image()
}

fn prepare_frame_yuv(img: &RgbaImage, w: u32, h: u32, blur: f32) -> Array3<u8> {
    let cropped = crop_center(&img, w, h);
    let blurred = image::imageops::blur(&cropped, blur);

    let mut yuv = Array3::<u8>::zeros((h as usize, w as usize, 3));

    for y in 0..h {
        for x in 0..w {
            let p = blurred.get_pixel(x, y);
            // convert RGBA -> YUV
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;

            let y_val = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
            let u_val = (128.0 + (-0.168736 * r - 0.331264 * g + 0.5 * b)).round() as u8;
            let v_val = (128.0 + (0.5 * r - 0.418688 * g - 0.081312 * b)).round() as u8;

            yuv[[y as usize, x as usize, 0]] = y_val;
            yuv[[y as usize, x as usize, 1]] = u_val;
            yuv[[y as usize, x as usize, 2]] = v_val;
        }
    }
    yuv
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
    let (dx, dy) = (x - px, y - py);

    let mut u1: f64 = 0.0;
    let mut u2: f64 = 1.0;

    for (p, q) in [(-dx, px), (dx, 1.0 - px), (-dy, py), (dy, 1.0 - py)] {
        if p == 0.0 && q < 0.0 {
            return None;
        }

        let t = q / p;

        if p < 0.0 {
            u1 = u1.max(t);
        } else if p > 0.0 {
            u2 = u2.min(t);
        }
    }

    if u1 > u2 {
        None
    } else {
        Some((px + u1 * dx, py + u1 * dy, px + u2 * dx, py + u2 * dy))
    }
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
