//! PyO3 bindings for fovea-mv.
//!
//! Surfaces the Step 4 public API: `Stream`, `Event`, the three trigger
//! classes, and `RegionMask`. The build artefact is loaded by the
//! `fovea_mv` Python package as `fovea_mv._fovea_mv`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use numpy::ndarray::Array3;
use numpy::{IntoPyArray, PyArray3};
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyType;

use fovea_mv_core::triggers::{
    FusionMode as RsFusionMode, FusionTrigger as RsFusionTrigger, IntervalTrigger as RsIntervalTrigger,
    MotionTrigger as RsMotionTrigger, RegionMask as RsRegionMask, SceneChangeTrigger as RsSceneChangeTrigger,
    SpatialClusterTrigger as RsSpatialClusterTrigger,
};
use fovea_mv_core::Trigger as RsTrigger;
use fovea_mv_stream::{FfmpegSource, NetworkOptions, OpenOptions, RtspTransport, SourceError};

/// Convert a fovea-mv-stream error into the most idiomatic Python exception.
fn err_to_py(e: SourceError) -> PyErr {
    match e {
        SourceError::ReadTimeout => PyTimeoutError::new_err("live source read timed out"),
        SourceError::Disconnected => PyConnectionError::new_err("live source disconnected"),
        other => PyRuntimeError::new_err(format!("{other}")),
    }
}

/// Internal handle to a trigger boxed as a trait object.
///
/// Each Python trigger class wraps one of these and feeds it through
/// `RsTrigger::evaluate` from inside `Stream`.
enum TriggerImpl {
    Motion(RsMotionTrigger),
    Interval(RsIntervalTrigger),
    SceneChange(RsSceneChangeTrigger),
    SpatialCluster(RsSpatialClusterTrigger),
    Fusion(RsFusionTrigger),
}

impl TriggerImpl {
    fn evaluate(&mut self, packet: &fovea_mv_core::MvPacket) -> Option<fovea_mv_core::Event> {
        match self {
            Self::Motion(t) => t.evaluate(packet),
            Self::Interval(t) => t.evaluate(packet),
            Self::SceneChange(t) => t.evaluate(packet),
            Self::SpatialCluster(t) => t.evaluate(packet),
            Self::Fusion(t) => t.evaluate(packet),
        }
    }
}

/// Region mask for `MotionTrigger`.
#[pyclass(name = "RegionMask")]
#[derive(Clone, Copy)]
struct PyRegionMask {
    inner: RsRegionMask,
}

#[pymethods]
impl PyRegionMask {
    #[new]
    fn new(x0: i16, y0: i16, x1: i16, y1: i16) -> Self {
        Self {
            inner: RsRegionMask { x0, y0, x1, y1 },
        }
    }

    #[getter]
    fn x0(&self) -> i16 { self.inner.x0 }
    #[getter]
    fn y0(&self) -> i16 { self.inner.y0 }
    #[getter]
    fn x1(&self) -> i16 { self.inner.x1 }
    #[getter]
    fn y1(&self) -> i16 { self.inner.y1 }

    fn __repr__(&self) -> String {
        format!(
            "RegionMask(x0={}, y0={}, x1={}, y1={})",
            self.inner.x0, self.inner.y0, self.inner.x1, self.inner.y1
        )
    }
}

/// Fires once per above-threshold motion-energy episode.
///
/// Two threshold modes:
/// - `MotionTrigger(absolute_threshold)`: compare against
///   `motion_energy(packet.mvs)`. Frame-area dependent.
/// - `MotionTrigger.per_mb(threshold_per_mb)`: compare against
///   `motion_energy / packet.total_mb`. Resolution-independent and the
///   recommended path for cross-clip configurations.
#[pyclass(name = "MotionTrigger")]
struct PyMotionTrigger {
    absolute_threshold: Option<u64>,
    per_mb_threshold: Option<f32>,
    min_duration_ms: u32,
    mask: Option<PyRegionMask>,
}

#[pymethods]
impl PyMotionTrigger {
    #[new]
    #[pyo3(signature = (energy_threshold, *, min_duration_ms = 0, mask = None))]
    fn new(energy_threshold: u64, min_duration_ms: u32, mask: Option<PyRegionMask>) -> Self {
        Self {
            absolute_threshold: Some(energy_threshold),
            per_mb_threshold: None,
            min_duration_ms,
            mask,
        }
    }

    /// Construct with a per-macroblock energy threshold.
    ///
    /// Resolution-independent. Convert from an absolute `T` known to work
    /// on a `W × H` clip: ``T / ceil(W/16) / ceil(H/16)``.
    #[classmethod]
    #[pyo3(signature = (threshold_per_mb, *, min_duration_ms = 0, mask = None))]
    fn per_mb(
        _cls: &Bound<'_, PyType>,
        threshold_per_mb: f32,
        min_duration_ms: u32,
        mask: Option<PyRegionMask>,
    ) -> Self {
        Self {
            absolute_threshold: None,
            per_mb_threshold: Some(threshold_per_mb),
            min_duration_ms,
            mask,
        }
    }

    fn __repr__(&self) -> String {
        match (self.absolute_threshold, self.per_mb_threshold) {
            (Some(t), _) => format!(
                "MotionTrigger(energy_threshold={}, min_duration_ms={})",
                t, self.min_duration_ms
            ),
            (_, Some(t)) => format!(
                "MotionTrigger.per_mb(threshold_per_mb={}, min_duration_ms={})",
                t, self.min_duration_ms
            ),
            _ => "MotionTrigger(<unset>)".into(),
        }
    }
}

/// Heartbeat trigger.
#[pyclass(name = "IntervalTrigger")]
struct PyIntervalTrigger {
    max_gap_ms: u32,
}

#[pymethods]
impl PyIntervalTrigger {
    #[new]
    fn new(max_gap_ms: u32) -> Self {
        Self { max_gap_ms }
    }

    fn __repr__(&self) -> String {
        format!("IntervalTrigger(max_gap_ms={})", self.max_gap_ms)
    }
}

/// Scene-change trigger.
#[pyclass(name = "SceneChangeTrigger")]
struct PySceneChangeTrigger {
    intra_block_ratio_threshold: f32,
}

#[pymethods]
impl PySceneChangeTrigger {
    #[new]
    fn new(intra_block_ratio_threshold: f32) -> Self {
        Self {
            intra_block_ratio_threshold,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "SceneChangeTrigger(intra_block_ratio_threshold={})",
            self.intra_block_ratio_threshold
        )
    }
}

/// Spatial-cluster trigger.
///
/// Fires when motion energy is concentrated in a small region of the
/// frame. Suppresses motion that is uniformly scattered (leaves, rain,
/// flickering background).
///
/// Two thresholds:
/// - `min_total_energy`: minimum sum of MV magnitudes per packet to
///   even consider firing (noise floor).
/// - `min_concentration`: minimum `peak_cell_energy / total_energy`
///   ratio in [0.0, 1.0]. 0.5 means "at least half the energy lives
///   in a single grid cell".
///
/// Optional `cell_size_px` (default 128) sets the spatial grid
/// resolution and `cooldown_ms` (default 100) the per-fire debounce.
#[pyclass(name = "SpatialClusterTrigger")]
struct PySpatialClusterTrigger {
    min_total_energy: u64,
    min_concentration: f32,
    cell_size_px: u16,
    cooldown_ms: u32,
}

#[pymethods]
impl PySpatialClusterTrigger {
    #[new]
    #[pyo3(signature = (min_total_energy, min_concentration, *, cell_size_px=128, cooldown_ms=100))]
    fn new(min_total_energy: u64, min_concentration: f32, cell_size_px: u16, cooldown_ms: u32) -> Self {
        Self {
            min_total_energy,
            min_concentration,
            cell_size_px,
            cooldown_ms,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "SpatialClusterTrigger(min_total_energy={}, min_concentration={}, cell_size_px={}, cooldown_ms={})",
            self.min_total_energy, self.min_concentration, self.cell_size_px, self.cooldown_ms
        )
    }
}

/// Combined-signal trigger.
///
/// Wraps `FusionTrigger` from `fovea-mv-core`. Combines motion energy,
/// intra-block ratio, and skip ratio in a single decision. See the
/// Rust crate docs for the four-quadrant decision table.
///
/// Use `mode="any"` (recall-favoured) or `mode="all"` (precision).
/// `motion_threshold` / `intra_threshold` set the per-signal cutoffs;
/// pass `None` to ignore that signal in the decision.
/// `skip_suppress` is a hard idle gate — when the encoder marks at
/// least that fraction of the frame as MODE_SKIP (HEVC), the trigger
/// returns no event regardless of the other signals.
#[pyclass(name = "FusionTrigger")]
struct PyFusionTrigger {
    mode: RsFusionMode,
    motion_threshold: Option<u64>,
    intra_threshold: Option<f32>,
    skip_suppress: Option<f32>,
    cooldown_ms: u32,
}

#[pymethods]
impl PyFusionTrigger {
    #[new]
    #[pyo3(signature = (
        mode = "any",
        *,
        motion_threshold = None,
        intra_threshold = None,
        skip_suppress = None,
        cooldown_ms = 100,
    ))]
    fn new(
        mode: &str,
        motion_threshold: Option<u64>,
        intra_threshold: Option<f32>,
        skip_suppress: Option<f32>,
        cooldown_ms: u32,
    ) -> PyResult<Self> {
        let mode = match mode.to_ascii_lowercase().as_str() {
            "any" | "anyof" | "any_of" => RsFusionMode::AnyOf,
            "all" | "allof" | "all_of" => RsFusionMode::AllOf,
            other => {
                return Err(PyValueError::new_err(format!(
                    "FusionTrigger mode must be 'any' or 'all', got {other:?}"
                )))
            }
        };
        Ok(Self {
            mode,
            motion_threshold,
            intra_threshold,
            skip_suppress,
            cooldown_ms,
        })
    }

    fn __repr__(&self) -> String {
        let mode = match self.mode {
            RsFusionMode::AnyOf => "any",
            RsFusionMode::AllOf => "all",
        };
        format!(
            "FusionTrigger(mode={:?}, motion_threshold={:?}, intra_threshold={:?}, skip_suppress={:?}, cooldown_ms={})",
            mode, self.motion_threshold, self.intra_threshold, self.skip_suppress, self.cooldown_ms
        )
    }
}

/// Convert a Python trigger object into the internal `TriggerImpl`.
fn build_trigger(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<TriggerImpl> {
    if let Ok(t) = obj.extract::<PyRef<PyMotionTrigger>>() {
        let mut inner = match (t.absolute_threshold, t.per_mb_threshold) {
            (Some(abs), _) => RsMotionTrigger::new(abs),
            (_, Some(per)) => RsMotionTrigger::with_per_mb_threshold(per),
            _ => {
                return Err(PyValueError::new_err(
                    "MotionTrigger must have an absolute or per-MB threshold",
                ))
            }
        }
        .with_min_duration_ms(t.min_duration_ms);
        if let Some(m) = t.mask {
            inner = inner.with_mask(m.inner);
        }
        Ok(TriggerImpl::Motion(inner))
    } else if let Ok(t) = obj.extract::<PyRef<PyIntervalTrigger>>() {
        Ok(TriggerImpl::Interval(RsIntervalTrigger::new(t.max_gap_ms)))
    } else if let Ok(t) = obj.extract::<PyRef<PySceneChangeTrigger>>() {
        Ok(TriggerImpl::SceneChange(RsSceneChangeTrigger::new(
            t.intra_block_ratio_threshold,
        )))
    } else if let Ok(t) = obj.extract::<PyRef<PySpatialClusterTrigger>>() {
        Ok(TriggerImpl::SpatialCluster(
            RsSpatialClusterTrigger::new(t.min_total_energy, t.min_concentration)
                .with_cell_size_px(t.cell_size_px)
                .with_cooldown_ms(t.cooldown_ms),
        ))
    } else if let Ok(t) = obj.extract::<PyRef<PyFusionTrigger>>() {
        let mut inner = RsFusionTrigger::new(t.mode).with_cooldown_ms(t.cooldown_ms);
        if let Some(v) = t.motion_threshold {
            inner = inner.with_motion_threshold(v);
        }
        if let Some(v) = t.intra_threshold {
            inner = inner.with_intra_threshold(v);
        }
        if let Some(v) = t.skip_suppress {
            inner = inner.with_skip_suppress(v);
        }
        Ok(TriggerImpl::Fusion(inner))
    } else {
        let _ = py;
        Err(PyValueError::new_err(
            "expected MotionTrigger / IntervalTrigger / SceneChangeTrigger / SpatialClusterTrigger / FusionTrigger",
        ))
    }
}

/// Triggered event.
///
/// Pure metadata; pixel data is fetched separately via `decode()`.
///
/// Marked `unsendable` because the back-reference into the FFmpeg source
/// is not Send/Sync; the underlying `AVFormatContext` and decoder hold
/// thread-bound state.
#[pyclass(name = "Event", unsendable)]
struct PyEvent {
    ts_us: i64,
    frame_type: char,
    trigger_name: &'static str,
    energy: u64,
    intra_ratio: f32,
    skip_ratio: f32,
    cbf_density: f32,
    mv_count: u32,
    /// Back-reference to the source so `decode()` can pull RGB. The reference
    /// is valid only until the source advances or is dropped — see the
    /// `decode()` docstring.
    source: Arc<Mutex<FfmpegSource>>,
    /// Sequence number of this event's packet within the stream. Used to
    /// detect "user advanced past us" before calling decode().
    seq: u64,
    /// The seq the source is currently parked on. We compare against this on
    /// decode().
    source_current_seq: Arc<Mutex<u64>>,
}

#[pymethods]
impl PyEvent {
    #[getter]
    fn timestamp_s(&self) -> f64 {
        self.ts_us as f64 / 1_000_000.0
    }

    #[getter]
    fn ts_us(&self) -> i64 {
        self.ts_us
    }

    #[getter]
    fn frame_type(&self) -> String {
        self.frame_type.to_string()
    }

    #[getter]
    fn trigger_name(&self) -> &'static str {
        self.trigger_name
    }

    #[getter]
    fn energy(&self) -> u64 {
        self.energy
    }

    #[getter]
    fn intra_ratio(&self) -> f32 {
        self.intra_ratio
    }

    /// Fraction of macroblocks the encoder coded as MODE_SKIP (HEVC
    /// only; H.264 leaves this at 0.0). High values are a direct
    /// "this region didn't change" signal — useful as an idle
    /// confidence indicator.
    #[getter]
    fn skip_ratio(&self) -> f32 {
        self.skip_ratio
    }

    /// Fraction of macroblocks whose transform unit carried at least
    /// one non-zero residual coefficient (HEVC only; H.264 leaves
    /// this at 0.0). Frame-level Coded Block Flag summary —
    /// orthogonal to `intra_ratio` / `skip_ratio`. Captures "how
    /// many bits did the encoder spend on residual data" without
    /// reading the residual coefficients themselves.
    #[getter]
    fn cbf_density(&self) -> f32 {
        self.cbf_density
    }

    #[getter]
    fn mv_count(&self) -> u32 {
        self.mv_count
    }

    /// Return the decoded RGB frame as a `(H, W, 3)` `numpy.ndarray` of
    /// `uint8`.
    ///
    /// Only valid until the iterator advances past this event. Calling after
    /// advancing raises `RuntimeError`. Repeated calls on the same event are
    /// allowed and return a fresh ndarray each time.
    fn decode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray3<u8>>> {
        let cur = *self
            .source_current_seq
            .lock()
            .map_err(|_| PyRuntimeError::new_err("source mutex poisoned"))?;
        if cur != self.seq {
            return Err(PyRuntimeError::new_err(
                "Event.decode() called after the iterator advanced past this event",
            ));
        }
        let mut src = self
            .source
            .lock()
            .map_err(|_| PyRuntimeError::new_err("source mutex poisoned"))?;
        let info = src.info().clone();
        let rgb = src.last_frame_rgb().map_err(err_to_py)?;
        let arr = Array3::from_shape_vec(
            (info.height as usize, info.width as usize, 3),
            rgb,
        )
        .map_err(|e| PyRuntimeError::new_err(format!("ndarray shape error: {e}")))?;
        Ok(arr.into_pyarray(py))
    }

    fn __repr__(&self) -> String {
        format!(
            "Event(timestamp_s={:.3}, frame_type='{}', trigger='{}', energy={}, intra_ratio={:.3}, skip_ratio={:.3}, cbf_density={:.3}, mv_count={})",
            self.timestamp_s(),
            self.frame_type,
            self.trigger_name,
            self.energy,
            self.intra_ratio,
            self.skip_ratio,
            self.cbf_density,
            self.mv_count,
        )
    }
}

/// Read-only metadata about the open stream.
#[pyclass(name = "VideoInfo")]
struct PyVideoInfo {
    width: u32,
    height: u32,
    frame_rate_num: i32,
    frame_rate_den: i32,
    duration_s: Option<f64>,
}

#[pymethods]
impl PyVideoInfo {
    #[getter]
    fn width(&self) -> u32 { self.width }
    #[getter]
    fn height(&self) -> u32 { self.height }
    #[getter]
    fn frame_rate(&self) -> f64 {
        if self.frame_rate_den == 0 {
            0.0
        } else {
            self.frame_rate_num as f64 / self.frame_rate_den as f64
        }
    }
    #[getter]
    fn duration_s(&self) -> Option<f64> { self.duration_s }

    fn __repr__(&self) -> String {
        format!(
            "VideoInfo(width={}, height={}, frame_rate={:.3}, duration_s={:?})",
            self.width,
            self.height,
            self.frame_rate(),
            self.duration_s
        )
    }
}

/// File-backed video source. Yields `Event` objects through `events(...)`.
///
/// `unsendable` because the underlying FFmpeg context is not thread-safe.
#[pyclass(name = "Stream", unsendable)]
struct PyStream {
    source: Arc<Mutex<FfmpegSource>>,
    /// Monotonic packet sequence — incremented on every successful
    /// `next_packet`. Events captured at sequence N can `decode()` only while
    /// the source is still parked on N.
    current_seq: Arc<Mutex<u64>>,
}

#[pymethods]
impl PyStream {
    /// Open a file.
    ///
    /// `fast_decode=True` enables FFmpeg's `skip_loop_filter=all` and
    /// `skip_idct=all` flags. On FFmpeg 8 / H.264 this is roughly a 1.20×
    /// release-mode decode speedup; motion vectors are bit-exact identical
    /// either way. RGB output remains practically identical (mean-luma diff
    /// < 1/255), so enabling it has no downside for typical use.
    #[new]
    #[pyo3(signature = (path, *, fast_decode = false))]
    fn new(path: PathBuf, fast_decode: bool) -> PyResult<Self> {
        let opts = OpenOptions {
            skip_loop_filter_all: fast_decode,
            skip_idct_all: fast_decode,
        };
        let src = FfmpegSource::open_file_with(&path, opts).map_err(err_to_py)?;
        Ok(Self {
            source: Arc::new(Mutex::new(src)),
            current_seq: Arc::new(Mutex::new(0)),
        })
    }

    /// Classmethod alias for the constructor.
    #[classmethod]
    #[pyo3(signature = (path, *, fast_decode = false))]
    fn from_file(_cls: &Bound<'_, PyType>, path: PathBuf, fast_decode: bool) -> PyResult<Self> {
        Self::new(path, fast_decode)
    }

    /// Open a URL (rtsp://, rtsps://, rtmp://, http(s)://, file://, ...).
    ///
    /// On a live source (rtsp / rtmp / srt / udp / rtp), `next_packet` raises
    /// `ConnectionError` on disconnect and `TimeoutError` on read timeout.
    /// `max_reconnects > 0` enables automatic reopen on those errors.
    #[classmethod]
    #[pyo3(signature = (
        url, *,
        transport = "tcp",
        open_timeout_s = 5.0,
        read_timeout_s = 5.0,
        max_reconnects = 0u32,
        fast_decode = false,
    ))]
    fn from_url(
        _cls: &Bound<'_, PyType>,
        url: &str,
        transport: &str,
        open_timeout_s: f64,
        read_timeout_s: f64,
        max_reconnects: u32,
        fast_decode: bool,
    ) -> PyResult<Self> {
        let transport = match transport.to_ascii_lowercase().as_str() {
            "tcp" => RtspTransport::Tcp,
            "udp" => RtspTransport::Udp,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown transport {other:?}; expected 'tcp' or 'udp'"
                )))
            }
        };
        if open_timeout_s < 0.0 || read_timeout_s < 0.0 {
            return Err(PyValueError::new_err("timeouts must be non-negative"));
        }
        let net = NetworkOptions {
            transport,
            open_timeout_ms: (open_timeout_s * 1000.0) as u32,
            read_timeout_ms: (read_timeout_s * 1000.0) as u32,
            max_reconnects,
        };
        let dec = OpenOptions {
            skip_loop_filter_all: fast_decode,
            skip_idct_all: fast_decode,
        };
        let src = FfmpegSource::open_url_with(url, net, dec).map_err(err_to_py)?;
        Ok(Self {
            source: Arc::new(Mutex::new(src)),
            current_seq: Arc::new(Mutex::new(0)),
        })
    }

    /// Read-only metadata.
    fn info(&self) -> PyResult<PyVideoInfo> {
        let src = self
            .source
            .lock()
            .map_err(|_| PyRuntimeError::new_err("source mutex poisoned"))?;
        let info = src.info();
        Ok(PyVideoInfo {
            width: info.width,
            height: info.height,
            frame_rate_num: info.frame_rate.0,
            frame_rate_den: info.frame_rate.1,
            duration_s: info.duration_us.map(|d| d as f64 / 1_000_000.0),
        })
    }

    /// Iterate events produced by the supplied trigger list.
    fn events(&self, py: Python<'_>, triggers: Vec<Py<PyAny>>) -> PyResult<PyEventIterator> {
        let mut impls: Vec<TriggerImpl> = Vec::with_capacity(triggers.len());
        for obj in triggers {
            let bound = obj.bind(py).clone();
            impls.push(build_trigger(py, &bound)?);
        }
        Ok(PyEventIterator {
            source: self.source.clone(),
            current_seq: self.current_seq.clone(),
            triggers: impls,
            exhausted: false,
        })
    }
}

/// Iterator yielded by `Stream.events()`.
///
/// `unsendable` for the same reason as [`PyStream`].
#[pyclass(name = "EventIterator", unsendable)]
struct PyEventIterator {
    source: Arc<Mutex<FfmpegSource>>,
    current_seq: Arc<Mutex<u64>>,
    triggers: Vec<TriggerImpl>,
    exhausted: bool,
}

#[pymethods]
impl PyEventIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<PyEvent>> {
        if self.exhausted {
            return Ok(None);
        }
        loop {
            // Advance to the next packet.
            let (seq, fired) = {
                let mut src = self
                    .source
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("source mutex poisoned"))?;
                let pkt = src.next_packet().map_err(err_to_py)?;
                let Some(pkt) = pkt else {
                    self.exhausted = true;
                    return Ok(None);
                };
                let mut seq_lock = self
                    .current_seq
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("seq mutex poisoned"))?;
                *seq_lock += 1;
                let seq = *seq_lock;

                // Evaluate triggers in order; first to fire wins.
                let mut fired_event: Option<fovea_mv_core::Event> = None;
                for t in self.triggers.iter_mut() {
                    if let Some(ev) = t.evaluate(pkt) {
                        fired_event = Some(ev);
                        break;
                    }
                }
                (seq, fired_event)
            };

            if let Some(ev) = fired {
                return Ok(Some(PyEvent {
                    ts_us: ev.ts_us,
                    frame_type: ev.frame_type.as_char(),
                    trigger_name: ev.trigger_name,
                    energy: ev.energy,
                    intra_ratio: ev.intra_ratio,
                    skip_ratio: ev.skip_ratio,
                    cbf_density: ev.cbf_density,
                    mv_count: ev.mv_count,
                    source: self.source.clone(),
                    seq,
                    source_current_seq: self.current_seq.clone(),
                }));
            }
        }
    }
}

#[pyfunction]
fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _fovea_mv(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(core_version, m)?)?;
    m.add_class::<PyStream>()?;
    m.add_class::<PyEventIterator>()?;
    m.add_class::<PyEvent>()?;
    m.add_class::<PyVideoInfo>()?;
    m.add_class::<PyMotionTrigger>()?;
    m.add_class::<PyIntervalTrigger>()?;
    m.add_class::<PySceneChangeTrigger>()?;
    m.add_class::<PySpatialClusterTrigger>()?;
    m.add_class::<PyFusionTrigger>()?;
    m.add_class::<PyRegionMask>()?;
    Ok(())
}
