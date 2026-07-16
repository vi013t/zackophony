use std::{
	collections::HashMap,
	io::Cursor,
	num::NonZero,
	sync::{Arc, Mutex},
	time::Duration,
};

use ringbuf::{
	HeapCons,
	HeapRb,
	traits::{Consumer as _, Producer, Split as _},
};
use rodio::{
	ChannelCount,
	Decoder,
	DeviceSinkBuilder,
	DeviceTrait as _,
	MixerDeviceSink,
	SampleRate,
	Source,
	cpal::{
		Stream,
		traits::{HostTrait as _, StreamTrait as _},
	},
	source::Buffered,
};

#[derive(Clone)]
pub struct SoundData {
	data: Buffered<Decoder<Cursor<Vec<u8>>>>,
}

impl SoundData {
	pub fn new(bytes: Vec<u8>) -> anyhow::Result<SoundData> {
		Ok(SoundData {
			data: Decoder::try_from(Cursor::new(bytes))?.buffered(),
		})
	}
}

pub struct AudioPlayer {
	cable_sink: MixerDeviceSink,
	monitor_sink: MixerDeviceSink,

	ui_sounds: HashMap<UiSound, Buffered<Decoder<Cursor<&'static [u8]>>>>,
	user_sounds: HashMap<String, SoundData>,

	_mic_level: Arc<Mutex<f32>>,
	_mic_stream: Stream,
	_mic_consumer: Arc<Mutex<HeapCons<f32>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UiSound {
	Hover,
}

impl UiSound {
	fn bytes(&self) -> &'static [u8] {
		match self {
			UiSound::Hover => include_bytes!("../assets/sounds/click.mp3").as_slice(),
		}
	}

	fn buffered(&self) -> anyhow::Result<Buffered<Decoder<Cursor<&'static [u8]>>>> {
		Ok(Decoder::try_from(Cursor::new(self.bytes()))?.buffered())
	}
}

impl AudioPlayer {
	pub fn new() -> anyhow::Result<AudioPlayer> {
		let host = rodio::cpal::default_host();

		let cable_device = host
			.output_devices()?
			.find(|d| d.description().is_ok_and(|d| d.name().contains("CABLE Input")))
			.ok_or_else(|| anyhow::anyhow!("VB-CABLE not found"))?;

		println!("Cable: {} Hz", cable_device.default_output_config().unwrap().sample_rate());

		let monitor_device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("No speakers found"))?;

		let cable_sink = DeviceSinkBuilder::from_device(cable_device)?.open_stream()?;
		let monitor_sink = DeviceSinkBuilder::from_device(monitor_device)?.open_stream()?;
		let mic_level = Arc::new(Mutex::new(0.0));

		let (mic_stream, mic_consumer) = AudioPlayer::start_microphone(mic_level.clone())?;

		let mic_source = MicSource {
			consumer: mic_consumer.clone(),
			sample_rate: 48000,
			last_sample: 0.0,
			left: false,
		};

		cable_sink.mixer().add(mic_source.clone());
		monitor_sink.mixer().add(mic_source);

		Ok(AudioPlayer {
			ui_sounds: HashMap::from([(UiSound::Hover, UiSound::Hover.buffered()?)]),
			cable_sink,
			monitor_sink,
			user_sounds: HashMap::new(),
			_mic_level: mic_level,
			_mic_stream: mic_stream,
			_mic_consumer: mic_consumer,
		})
	}

	fn add_to_outputs(&self, source: impl Source<Item = f32> + Clone + Send + 'static) {
		self.cable_sink.mixer().add(source.clone());

		self.monitor_sink.mixer().add(source);
	}

	pub fn upload_user_sound(&mut self, name: &str, data: SoundData) -> anyhow::Result<()> {
		self.user_sounds.insert(name.to_owned(), data);

		Ok(())
	}

	pub fn play_user_sound(&mut self, name: &str) -> anyhow::Result<()> {
		let source = self.user_sounds.get(name).ok_or_else(|| anyhow::anyhow!("Missing sound: {name}"))?;
		self.add_to_outputs(source.data.clone());
		Ok(())
	}

	pub fn play_ui_sound(&self, sound: UiSound) -> anyhow::Result<()> {
		self.monitor_sink.mixer().add(self.ui_sounds.get(&sound).unwrap().clone().amplify(0.01));
		Ok(())
	}

	fn start_microphone(level: Arc<Mutex<f32>>) -> anyhow::Result<(Stream, Arc<Mutex<ringbuf::HeapCons<f32>>>)> {
		let host = rodio::cpal::default_host();
		let device = host.default_input_device().ok_or_else(|| anyhow::anyhow!("No microphone"))?;
		let config = device.default_input_config()?;

		println!("Mic: {} Hz, {:?}", config.sample_rate(), config.sample_format());

		let rb = HeapRb::<f32>::new(48000);

		let (producer, consumer) = rb.split();
		let producer = Arc::new(Mutex::new(producer));
		let consumer = Arc::new(Mutex::new(consumer));

		let stream = match config.sample_format() {
			rodio::cpal::SampleFormat::F32 => Self::build_mic_stream::<f32>(&device, &config.into(), level, producer)?,
			rodio::cpal::SampleFormat::I16 => Self::build_mic_stream::<i16>(&device, &config.into(), level, producer)?,
			rodio::cpal::SampleFormat::U16 => Self::build_mic_stream::<u16>(&device, &config.into(), level, producer)?,
			_ => anyhow::bail!("Unsupported mic format"),
		};

		stream.play()?;

		Ok((stream, consumer))
	}

	fn build_mic_stream<T>(device: &rodio::cpal::Device, config: &rodio::cpal::StreamConfig, level: Arc<Mutex<f32>>, producer: MicProducer) -> anyhow::Result<Stream>
	where
		T: rodio::cpal::SizedSample,
		f32: rodio::cpal::FromSample<T>,
	{
		Ok(device.build_input_stream(
			config,
			move |data: &[T], _| {
				let mut sum = 0.0;

				for sample in data {
					let sample: f32 = rodio::cpal::Sample::from_sample(*sample);
					sum += sample * sample;
					let _ = producer.lock().unwrap().try_push(sample);
				}

				let rms = (sum / data.len() as f32).sqrt();

				*level.lock().unwrap() = rms;
			},
			move |err| {
				eprintln!("Mic error: {}", err);
			},
			None,
		)?)
	}
}

type MicConsumer = Arc<Mutex<ringbuf::HeapCons<f32>>>;
type MicProducer = Arc<Mutex<ringbuf::HeapProd<f32>>>;

#[derive(Clone)]
struct MicSource {
	consumer: MicConsumer,
	sample_rate: u32,
	last_sample: f32,
	left: bool,
}

impl Iterator for MicSource {
	type Item = f32;

	fn next(&mut self) -> Option<f32> {
		if self.left {
			self.left = false;

			Some(self.last_sample)
		} else {
			self.left = true;

			let sample = self.consumer.lock().unwrap().try_pop().unwrap_or(0.0);

			self.last_sample = sample;

			Some(sample)
		}
	}
}

impl Source for MicSource {
	fn current_span_len(&self) -> Option<usize> {
		None
	}

	fn channels(&self) -> ChannelCount {
		NonZero::new(2).unwrap()
	}

	fn sample_rate(&self) -> SampleRate {
		NonZero::new(self.sample_rate).unwrap()
	}

	fn total_duration(&self) -> Option<Duration> {
		None
	}
}
