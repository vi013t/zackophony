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
	Device,
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

use crate::soundboard::Sound;

#[derive(Clone)]
pub struct SoundData {
	bytes: Vec<u8>,
	data: Buffered<Decoder<Cursor<Vec<u8>>>>,
}

impl SoundData {
	pub fn new(bytes: Vec<u8>) -> anyhow::Result<SoundData> {
		Ok(SoundData {
			bytes: bytes.clone(),
			data: Decoder::try_from(Cursor::new(bytes))?.buffered(),
		})
	}

	pub fn bytes(&self) -> &[u8] {
		&self.bytes
	}
}

pub struct AudioManager {
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

impl AudioManager {
	pub fn set_devices(&mut self, input_device: Device, output_device: Device, virtual_device: Device) {
		let user_sounds = std::mem::take(&mut self.user_sounds);
		let ui_sounds = std::mem::take(&mut self.ui_sounds);
		if let Ok(manager) = AudioManager::with_sounds(input_device, output_device, virtual_device, user_sounds, ui_sounds) {
			*self = manager;
		}
	}

	fn with_sounds(
		input_device: Device,
		monitor_device: Device,
		cable_device: Device,
		user_sounds: HashMap<String, SoundData>,
		ui_sounds: HashMap<UiSound, Buffered<Decoder<Cursor<&'static [u8]>>>>,
	) -> anyhow::Result<AudioManager> {
		let cable_sink = DeviceSinkBuilder::from_device(cable_device)?.open_stream()?;
		let monitor_sink = DeviceSinkBuilder::from_device(monitor_device)?.open_stream()?;
		let mic_level = Arc::new(Mutex::new(0.0));

		let (mic_stream, mic_consumer, sample_rate) = AudioManager::start_microphone(input_device, mic_level.clone())?;

		let mic_source = MicSource {
			consumer: mic_consumer.clone(),
			sample_rate,
			last_sample: 0.0,
			left: false,
		};

		cable_sink.mixer().add(mic_source.clone());

		// uncomment this to enable mic sidetone
		// monitor_sink.mixer().add(mic_source);

		Ok(AudioManager {
			ui_sounds,
			cable_sink,
			monitor_sink,
			user_sounds,
			_mic_level: mic_level,
			_mic_stream: mic_stream,
			_mic_consumer: mic_consumer,
		})
	}

	pub fn new(input_device: Device, monitor_device: Device, cable_device: Device) -> anyhow::Result<AudioManager> {
		let cable_sink = DeviceSinkBuilder::from_device(cable_device)?.open_stream()?;
		let monitor_sink = DeviceSinkBuilder::from_device(monitor_device)?.open_stream()?;
		let mic_level = Arc::new(Mutex::new(0.0));

		let (mic_stream, mic_consumer, sample_rate) = AudioManager::start_microphone(input_device, mic_level.clone())?;

		let mic_source = MicSource {
			consumer: mic_consumer.clone(),
			sample_rate,
			last_sample: 0.0,
			left: false,
		};

		cable_sink.mixer().add(mic_source.clone());

		// uncomment this to enable mic sidetone
		// monitor_sink.mixer().add(mic_source);

		Ok(AudioManager {
			ui_sounds: HashMap::from([(UiSound::Hover, UiSound::Hover.buffered()?)]),
			cable_sink,
			monitor_sink,
			user_sounds: HashMap::new(),
			_mic_level: mic_level,
			_mic_stream: mic_stream,
			_mic_consumer: mic_consumer,
		})
	}

	fn play_to_others(&self, source: impl Source<Item = f32> + Clone + Send + 'static, volume: f32) {
		self.cable_sink.mixer().add(source.clone().amplify(volume));
	}

	fn play_to_me(&self, source: impl Source<Item = f32> + Clone + Send + 'static, volume: f32) {
		self.monitor_sink.mixer().add(source.clone().amplify(volume));
	}

	pub fn upload_user_sound(&mut self, name: &str, data: SoundData) -> anyhow::Result<()> {
		self.user_sounds.insert(name.to_owned(), data);

		Ok(())
	}

	pub fn play_user_sound(&mut self, sound: &Sound) -> anyhow::Result<()> {
		self.play_to_me(sound.data.data.clone(), sound.volume_for_me as f32 / 100.);
		self.play_to_others(sound.data.data.clone(), sound.volume_for_others as f32 / 100.);
		Ok(())
	}

	pub fn play_ui_sound(&self, sound: UiSound) -> anyhow::Result<()> {
		self.monitor_sink.mixer().add(self.ui_sounds.get(&sound).unwrap().clone().amplify(0.01));
		Ok(())
	}

	pub fn output_devices() -> Vec<Device> {
		let host = rodio::cpal::default_host();
		host.output_devices().map(|devices| devices.collect()).unwrap_or_else(|_| Vec::new())
	}

	pub fn default_virtual_device() -> Option<Device> {
		let host = rodio::cpal::default_host();
		host.output_devices()
			.ok()
			.and_then(|mut devices| devices.find(|d| d.description().is_ok_and(|d| d.name().contains("CABLE Input"))))
	}

	pub fn default_input_device() -> Option<Device> {
		let host = rodio::cpal::default_host();
		host.default_input_device()
	}

	pub fn default_output_device() -> Option<Device> {
		let host = rodio::cpal::default_host();
		host.default_output_device()
	}

	pub fn input_devices() -> Vec<Device> {
		let host = rodio::cpal::default_host();
		host.input_devices().map(|devices| devices.collect()).unwrap_or_else(|_| Vec::new())
	}

	fn start_microphone(microphone: Device, level: Arc<Mutex<f32>>) -> anyhow::Result<(Stream, MicConsumer, u32)> {
		let config = microphone.default_input_config()?;
		let sample_rate = config.sample_rate();

		let rb = HeapRb::<f32>::new(48000);

		let (producer, consumer) = rb.split();
		let producer = Arc::new(Mutex::new(producer));
		let consumer = Arc::new(Mutex::new(consumer));

		let stream = match config.sample_format() {
			rodio::cpal::SampleFormat::F32 => Self::build_mic_stream::<f32>(&microphone, &config.into(), level, producer)?,
			rodio::cpal::SampleFormat::I16 => Self::build_mic_stream::<i16>(&microphone, &config.into(), level, producer)?,
			rodio::cpal::SampleFormat::U16 => Self::build_mic_stream::<u16>(&microphone, &config.into(), level, producer)?,
			_ => anyhow::bail!("Unsupported mic format"),
		};

		stream.play()?;

		Ok((stream, consumer, sample_rate))
	}

	fn build_mic_stream<T>(microphone: &rodio::cpal::Device, config: &rodio::cpal::StreamConfig, level: Arc<Mutex<f32>>, producer: MicProducer) -> anyhow::Result<Stream>
	where
		T: rodio::cpal::SizedSample,
		f32: rodio::cpal::FromSample<T>,
	{
		Ok(microphone.build_input_stream(
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
