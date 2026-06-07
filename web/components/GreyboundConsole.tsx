"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { ampControls, defaultRuntimeConfig, rigPresets, type AmpControlId, type Pedal, type RuntimeConfig } from "../lib/rigs";
import { formatDbfs, runtimePreview, type MonitorStats } from "../lib/simulation";
import { defaultTone3000Input, defaultTone3000Ir, tone3000Inputs, tone3000Irs } from "../lib/tone3000";
import { createWasmRenderState, renderWasmAudioBlock, type WasmRenderState } from "../lib/wasmMonitor";

const silentMonitorStats: MonitorStats = {
  inputRms: 0,
  inputPeak: 0,
  outputRms: 0,
  outputPeak: 0,
  inputNearClips: 0,
  inputClips: 0,
  outputNearClips: 0,
  outputClips: 0,
  inputOverruns: 0,
  outputUnderruns: 0,
  rails: {
    preampAvg: 0,
    preampMin: 0,
    piAvg: 0,
    piMin: 0,
    powerAvg: 0,
    powerMin: 0,
    screenAvg: 0,
    screenMin: 0,
  },
  currents: {
    firstAvg: 0,
    firstMax: 0,
    piAvg: 0,
    piMax: 0,
    powerAvg: 0,
    powerMax: 0,
    attackAvg: 0,
    attackMax: 0,
    screenAvg: 0,
    screenMax: 0,
  },
  cathodeAvg: 0,
  cathodeMax: 0,
  fluxAvg: 0,
  fluxMax: 0,
  probes: ["in", "amp", "tone", "send", "out"].map((label) => ({ label, avg: 0, max: 0 })),
};

function pedalBypassState(pedals: Pedal[]) {
  return Object.fromEntries(pedals.map((pedal) => [pedal.id, pedal.bypassed]));
}

function pedalControlState(pedals: Pedal[]) {
  return Object.fromEntries(pedals.map((pedal) => [pedal.id, pedal.controls]));
}

export function GreyboundConsole() {
  const [rigId, setRigId] = useState("nox30-all-pedals-bypassed");
  const [runtime, setRuntime] = useState<RuntimeConfig>({
    ...defaultRuntimeConfig,
    inputSourceUrl: defaultTone3000Input.url,
    irSourceUrl: defaultTone3000Ir.url,
  });
  const [stats, setStats] = useState<MonitorStats>(silentMonitorStats);
  const [engineStatus, setEngineStatus] = useState("loading wasm");
  const [playbackStatus, setPlaybackStatus] = useState<"stopped" | "starting" | "playing">("stopped");
  const renderStateRef = useRef<WasmRenderState | null>(null);
  const playbackRef = useRef<{
    context: AudioContext;
    intervalId: number;
    nextStartTime: number;
    sources: Set<AudioBufferSourceNode>;
  } | null>(null);
  const rig = useMemo(() => rigPresets.find((preset) => preset.id === rigId) ?? rigPresets[0], [rigId]);
  const [ampValues, setAmpValues] = useState(rig.amp);
  const [ampBypassed, setAmpBypassed] = useState(rig.ampBypassed);
  const [pedalBypass, setPedalBypass] = useState<Record<string, boolean>>(() => pedalBypassState(rig.pedals));
  const [pedalControls, setPedalControls] = useState<Record<string, Record<string, number | string>>>(() => pedalControlState(rig.pedals));
  const [selectedDeviceId, setSelectedDeviceId] = useState("amp");
  const liveRig = useMemo(
    () => ({
      ...rig,
      amp: ampValues,
      ampBypassed,
      pedals: rig.pedals.map((pedal) => ({
        ...pedal,
        bypassed: pedalBypass[pedal.id] ?? pedal.bypassed,
        controls: pedalControls[pedal.id] ?? pedal.controls,
      })),
    }),
    [ampBypassed, ampValues, pedalBypass, pedalControls, rig],
  );
  const liveRigRef = useRef(liveRig);
  const ampValuesRef = useRef(ampValues);
  const runtimeRef = useRef(runtime);
  const monitorStats = stats;

  useEffect(() => {
    setAmpValues(rig.amp);
    setAmpBypassed(rig.ampBypassed);
    setPedalBypass(pedalBypassState(rig.pedals));
    setPedalControls(pedalControlState(rig.pedals));
    setSelectedDeviceId("amp");
  }, [rig]);

  const togglePedalBypass = useCallback((pedalId: string) => {
    setPedalBypass((current) => ({
      ...current,
      [pedalId]: !current[pedalId],
    }));
  }, []);

  const setPedalControlValue = useCallback((pedalId: string, controlId: string, value: number | string) => {
    setPedalControls((current) => ({
      ...current,
      [pedalId]: {
        ...(current[pedalId] ?? {}),
        [controlId]: value,
      },
    }));
  }, []);

  useEffect(() => {
    liveRigRef.current = liveRig;
    ampValuesRef.current = ampValues;
    runtimeRef.current = runtime;
  }, [ampValues, liveRig, runtime]);

  const stopPlayback = useCallback(() => {
    const playback = playbackRef.current;
    if (!playback) {
      setPlaybackStatus("stopped");
      setStats(silentMonitorStats);
      return;
    }
    window.clearInterval(playback.intervalId);
    playback.sources.forEach((source) => {
      source.onended = null;
      try {
        source.stop();
      } catch {
        // Already stopped by the browser.
      }
    });
    playback.context.close().catch(() => undefined);
    playbackRef.current = null;
    setPlaybackStatus("stopped");
    setStats(silentMonitorStats);
  }, []);

  const schedulePlaybackBlocks = useCallback(() => {
    const playback = playbackRef.current;
    const state = renderStateRef.current;
    if (!playback || !state) return;

    const runtimeSnapshot = runtimeRef.current;
    const sampleRate = runtimeSnapshot.sampleRate;
    const blockSize = Math.max(1024, runtimeSnapshot.periodSize);
    const lookaheadSeconds = 0.35;

    while (playback.nextStartTime < playback.context.currentTime + lookaheadSeconds) {
      const block = renderWasmAudioBlock(
        state,
        liveRigRef.current,
        ampValuesRef.current,
        runtimeSnapshot,
        blockSize,
      );
      const audioBuffer = playback.context.createBuffer(1, block.output.length, sampleRate);
      audioBuffer.getChannelData(0).set(block.output);
      const source = playback.context.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(playback.context.destination);
      playback.sources.add(source);
      source.onended = () => {
        playback.sources.delete(source);
      };
      source.start(playback.nextStartTime);
      playback.nextStartTime += block.output.length / sampleRate;
      setStats(block.stats);
    }
  }, []);

  const startPlayback = useCallback(async () => {
    if (playbackRef.current || !renderStateRef.current) return;
    setPlaybackStatus("starting");
    const AudioContextCtor = window.AudioContext || window.webkitAudioContext;
    const context = new AudioContextCtor({ sampleRate: runtimeRef.current.sampleRate });
    await context.resume();
    playbackRef.current = {
      context,
      intervalId: window.setInterval(schedulePlaybackBlocks, 50),
      nextStartTime: context.currentTime + 0.06,
      sources: new Set(),
    };
    schedulePlaybackBlocks();
    setPlaybackStatus("playing");
  }, [schedulePlaybackBlocks]);

  useEffect(() => {
    let cancelled = false;
    stopPlayback();
    renderStateRef.current = null;
    setStats(silentMonitorStats);
    setEngineStatus("loading sources");
    createWasmRenderState({
      sampleRate: runtime.sampleRate,
      inputUrl: runtime.inputSourceUrl,
      irUrl: runtime.irSourceUrl,
      rig,
      outputGain: Math.pow(10, runtime.outputDb / 20),
    })
      .then((state) => {
        if (cancelled) {
          state.engine.free();
          return;
        }
        renderStateRef.current = state;
        setEngineStatus("wasm live");
      })
      .catch((error: unknown) => {
        renderStateRef.current = null;
        setEngineStatus(error instanceof Error ? `wasm fallback: ${error.message}` : "wasm fallback");
      });
    return () => {
      cancelled = true;
      renderStateRef.current?.engine.free();
      renderStateRef.current = null;
    };
  }, [rig, runtime.sampleRate, runtime.inputSourceUrl, runtime.irSourceUrl, stopPlayback]);

  useEffect(() => stopPlayback, [stopPlayback]);

  const runtimeDetails = runtimePreview(liveRig, runtime);

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">Greybound standalone</p>
          <h1>Monitor web</h1>
        </div>
        <div className="engineState">
          <span className="stateDot" />
          <span>{engineStatus}</span>
        </div>
      </header>

      <section className="workspace">
        <aside className="sidebar" aria-label="Rig presets">
          <label className="fieldLabel" htmlFor="rig-select">Rig</label>
          <select id="rig-select" value={rigId} onChange={(event) => setRigId(event.target.value)}>
            {rigPresets.map((preset) => (
              <option key={preset.id} value={preset.id}>{preset.name}</option>
            ))}
          </select>

          <div className="runtimeGrid">
            <NumberField label="Sample rate" value={runtime.sampleRate} min={1} step={1000} onChange={(sampleRate) => setRuntime({ ...runtime, sampleRate })} />
            <NumberField label="Period" value={runtime.periodSize} min={1} step={16} onChange={(periodSize) => setRuntime({ ...runtime, periodSize })} />
            <NumberField label="Input dB" value={runtime.inputDb} min={-60} max={24} step={1} onChange={(inputDb) => setRuntime({ ...runtime, inputDb })} />
            <NumberField label="Output dB" value={runtime.outputDb} min={-60} max={6} step={1} onChange={(outputDb) => setRuntime({ ...runtime, outputDb })} />
          </div>

          <div className="switches">
            <Switch label="Monitor" checked={runtime.monitor} onChange={(monitor) => setRuntime({ ...runtime, monitor })} />
            <Switch label="Speaker IR" checked={runtime.speakerIr} onChange={(speakerIr) => setRuntime({ ...runtime, speakerIr })} />
          </div>

          <AssetSelect
            label="TONE3000 input"
            value={runtime.inputSourceUrl}
            options={tone3000Inputs}
            onChange={(inputSourceUrl) => setRuntime({ ...runtime, inputSourceUrl })}
          />
          <AssetSelect
            label="TONE3000 IR"
            value={runtime.irSourceUrl}
            options={tone3000Irs}
            onChange={(irSourceUrl) => setRuntime({ ...runtime, irSourceUrl })}
          />
          <ReadOnlyField label="Device" value={runtime.device} />
        </aside>

        <section className="mainPanel">
          <MonitorHeader
            rigName={liveRig.name}
            file={liveRig.file}
            log={runtime.monitorLog}
            playbackStatus={playbackStatus}
            canPlay={engineStatus === "wasm live"}
            onStart={startPlayback}
            onStop={stopPlayback}
          />
          <Pedalboard
            pedals={liveRig.pedals}
            ampBypassed={liveRig.ampBypassed}
            cabEnabled={runtime.speakerIr}
            onTogglePedal={togglePedalBypass}
            onToggleAmp={() => setAmpBypassed((value) => !value)}
            onToggleCab={() => setRuntime((current) => ({ ...current, speakerIr: !current.speakerIr }))}
            selectedDeviceId={selectedDeviceId}
            onSelectDevice={setSelectedDeviceId}
          />
          <Meters stats={monitorStats} />
          <ComponentTelemetry stats={monitorStats} />
          <div className="lowerGrid">
            <DeviceControlsPanel
              selectedDeviceId={selectedDeviceId}
              rig={liveRig}
              ampValues={ampValues}
              onAmpChange={(id, value) => setAmpValues({ ...ampValues, [id]: value })}
              onPedalChange={setPedalControlValue}
            />
            <RuntimePreview details={runtimeDetails} />
          </div>
        </section>
      </section>
    </main>
  );
}

function MonitorHeader({
  rigName,
  file,
  log,
  playbackStatus,
  canPlay,
  onStart,
  onStop,
}: {
  rigName: string;
  file: string;
  log: string;
  playbackStatus: "stopped" | "starting" | "playing";
  canPlay: boolean;
  onStart: () => void;
  onStop: () => void;
}) {
  return (
    <div className="monitorHeader">
      <div>
        <p className="eyebrow">model nox30</p>
        <h2>{rigName}</h2>
      </div>
      <dl>
        <div><dt>rig</dt><dd>{file}</dd></div>
        <div><dt>log</dt><dd>{log}</dd></div>
      </dl>
      <div className="transport">
        <button type="button" onClick={playbackStatus === "playing" ? onStop : onStart} disabled={!canPlay || playbackStatus === "starting"}>
          {playbackStatus === "playing" ? "Stop" : playbackStatus === "starting" ? "Starting" : "Play"}
        </button>
        <span>{playbackStatus}</span>
      </div>
    </div>
  );
}

function Pedalboard({
  pedals,
  ampBypassed,
  cabEnabled,
  onTogglePedal,
  onToggleAmp,
  onToggleCab,
  selectedDeviceId,
  onSelectDevice,
}: {
  pedals: Pedal[];
  ampBypassed: boolean;
  cabEnabled: boolean;
  onTogglePedal: (pedalId: string) => void;
  onToggleAmp: () => void;
  onToggleCab: () => void;
  selectedDeviceId: string;
  onSelectDevice: (deviceId: string) => void;
}) {
  const sections = [
    { id: "pre", label: "GTR", out: "AMP", pedals: pedals.filter((pedal) => pedal.section === "pre") },
    { id: "fx", label: "SEND", out: "RETURN", pedals: pedals.filter((pedal) => pedal.section === "fx") },
    { id: "post", label: "AMP OUT", out: "OUT", pedals: pedals.filter((pedal) => pedal.section === "post") },
  ];

  return (
    <div className="pedalboard">
      {sections.map((section) => (
        <div key={section.id} className={section.pedals.length || section.id === "pre" ? "signalRow" : "signalRow empty"}>
          <span className="node">{section.label}</span>
          <span className="cable" />
          {section.pedals.map((pedal) => (
            <PedalBox
              key={pedal.id}
              pedal={pedal}
              selected={selectedDeviceId === pedal.id}
              onSelect={() => onSelectDevice(pedal.id)}
              onToggle={() => onTogglePedal(pedal.id)}
            />
          ))}
          {section.id === "pre" ? <AmpBox bypassed={ampBypassed} selected={selectedDeviceId === "amp"} onSelect={() => onSelectDevice("amp")} onToggle={onToggleAmp} /> : null}
          {section.id === "pre" ? <CabBox enabled={cabEnabled} onToggle={onToggleCab} /> : null}
          <span className="cable" />
          <span className="node">{section.out}</span>
        </div>
      ))}
    </div>
  );
}

function PedalBox({ pedal, selected, onSelect, onToggle }: { pedal: Pedal; selected: boolean; onSelect: () => void; onToggle: () => void }) {
  return (
    <article
      className={[pedal.bypassed ? "pedal bypassed" : "pedal", selected ? "selectedDevice" : ""].filter(Boolean).join(" ")}
      style={{ "--pedal-color": pedal.color } as CSSProperties}
      onClick={onSelect}
    >
      <div className="pedalLed" />
      <strong>{pedal.label}</strong>
      <span>{pedal.bypassed ? "bypass" : "active"}</span>
      <button type="button" aria-label={`${pedal.label} footswitch`} aria-pressed={!pedal.bypassed} onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }} />
    </article>
  );
}

function AmpBox({ bypassed, selected, onSelect, onToggle }: { bypassed: boolean; selected: boolean; onSelect: () => void; onToggle: () => void }) {
  return (
    <article className={[bypassed ? "ampBox bypassed" : "ampBox", selected ? "selectedDevice" : ""].filter(Boolean).join(" ")} onClick={onSelect}>
      <div className="pedalLed" />
      <strong>AMP Nox30</strong>
      <span>{bypassed ? "bypass" : "active"}</span>
      <button type="button" aria-label="Amp footswitch" aria-pressed={!bypassed} onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }} />
    </article>
  );
}

function CabBox({ enabled, onToggle }: { enabled: boolean; onToggle: () => void }) {
  return (
    <article className={enabled ? "cabBox" : "cabBox bypassed"}>
      <div className="pedalLed" />
      <strong>CAB IR</strong>
      <span>{enabled ? "active" : "bypass"}</span>
      <button type="button" aria-label="Cab IR footswitch" aria-pressed={enabled} onClick={onToggle} />
    </article>
  );
}

function Meters({ stats }: { stats: MonitorStats }) {
  return (
    <div className="meters">
      <Meter label="input" rms={stats.inputRms} peak={stats.inputPeak} near={stats.inputNearClips} clips={stats.inputClips} />
      <Meter label="output" rms={stats.outputRms} peak={stats.outputPeak} near={stats.outputNearClips} clips={stats.outputClips} />
      <div className="xrun">
        <span>xrun in/out</span>
        <strong>{stats.inputOverruns}/{stats.outputUnderruns}</strong>
      </div>
    </div>
  );
}

function Meter({ label, rms, peak, near, clips }: { label: string; rms: number; peak: number; near: number; clips: number }) {
  return (
    <div className="meter">
      <div className="meterLabel">
        <span>{label}</span>
        <strong>rms {formatDbfs(rms)} dBFS</strong>
        <em>peak {formatDbfs(peak)} dBFS near/clip {near}/{clips}</em>
      </div>
      <div className="bar"><span style={{ width: `${Math.min(100, Math.max(0, ((20 * Math.log10(rms) + 60) / 60) * 100))}%` }} /></div>
    </div>
  );
}

function ComponentTelemetry({ stats }: { stats: MonitorStats }) {
  return (
    <section className="telemetry">
      <div className="telemetryLine">
        <span>rails avg/min</span>
        <strong>pre {stats.rails.preampAvg.toFixed(0)}/{stats.rails.preampMin.toFixed(0)} V</strong>
        <strong>pi {stats.rails.piAvg.toFixed(0)}/{stats.rails.piMin.toFixed(0)} V</strong>
        <strong>pwr {stats.rails.powerAvg.toFixed(0)}/{stats.rails.powerMin.toFixed(0)} V</strong>
        <strong>scr {stats.rails.screenAvg.toFixed(0)}/{stats.rails.screenMin.toFixed(0)} V</strong>
      </div>
      <div className="telemetryLine">
        <span>I avg/max mA</span>
        <strong>first {stats.currents.firstAvg.toFixed(2)}/{stats.currents.firstMax.toFixed(2)}</strong>
        <strong>pi {stats.currents.piAvg.toFixed(2)}/{stats.currents.piMax.toFixed(2)}</strong>
        <strong>pwr {stats.currents.powerAvg.toFixed(1)}/{stats.currents.powerMax.toFixed(1)}</strong>
        <strong>atk {stats.currents.attackAvg.toFixed(1)}/{stats.currents.attackMax.toFixed(1)}</strong>
      </div>
      <div className="probeStrip">
        {stats.probes.map((probe) => (
          <span key={probe.label}>{probe.label} {probe.avg.toFixed(3)}/{probe.max.toFixed(3)}</span>
        ))}
      </div>
    </section>
  );
}

function DeviceControlsPanel({
  selectedDeviceId,
  rig,
  ampValues,
  onAmpChange,
  onPedalChange,
}: {
  selectedDeviceId: string;
  rig: typeof rigPresets[number];
  ampValues: Record<AmpControlId, number>;
  onAmpChange: (id: AmpControlId, value: number) => void;
  onPedalChange: (pedalId: string, controlId: string, value: number | string) => void;
}) {
  const selectedPedal = rig.pedals.find((pedal) => pedal.id === selectedDeviceId);
  if (!selectedPedal) {
    return <AmpControls values={ampValues} onChange={onAmpChange} />;
  }

  const controls = Object.entries(selectedPedal.controls);
  return (
    <section className="controlsPanel">
      <div className="panelTitle">
        <h3>{selectedPedal.label} controls</h3>
        <span>{selectedPedal.bypassed ? "bypass" : "active"}</span>
      </div>
      <div className="knobGrid">
        {controls.map(([controlId, value]) => (
          <PedalControl
            key={controlId}
            controlId={controlId}
            value={value}
            onChange={(nextValue) => onPedalChange(selectedPedal.id, controlId, nextValue)}
          />
        ))}
      </div>
    </section>
  );
}

function AmpControls({ values, onChange }: { values: Record<AmpControlId, number>; onChange: (id: AmpControlId, value: number) => void }) {
  return (
    <section className="controlsPanel">
      <div className="panelTitle">
        <h3>amp controls</h3>
        <span>0.0-10.0</span>
      </div>
      <div className="knobGrid">
        {ampControls.map((control) => (
          <label key={control.id} className="knob">
            <span>{control.label}</span>
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={values[control.id]}
              onChange={(event) => onChange(control.id, Number(event.target.value))}
            />
            <strong>{(values[control.id] * 10).toFixed(1)}</strong>
          </label>
        ))}
      </div>
    </section>
  );
}

function PedalControl({ controlId, value, onChange }: { controlId: string; value: number | string; onChange: (value: number | string) => void }) {
  const descriptor = pedalControlDescriptors[controlId] ?? { label: humanizeControlId(controlId), min: 0, max: 1, step: 0.01 };
  if (typeof value === "string") {
    const options = stringControlOptions[controlId] ?? [value];
    return (
      <label className="knob">
        <span>{descriptor.label}</span>
        <select value={value} onChange={(event) => onChange(event.target.value)}>
          {options.map((option) => (
            <option key={option} value={option}>{option}</option>
          ))}
        </select>
        <strong>{value}</strong>
      </label>
    );
  }

  return (
    <label className="knob">
      <span>{descriptor.label}</span>
      <input
        type="range"
        min={descriptor.min}
        max={descriptor.max}
        step={descriptor.step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <strong>{formatControlValue(value)}</strong>
    </label>
  );
}

const pedalControlDescriptors: Record<string, { label: string; min: number; max: number; step: number }> = {
  peak_reduction: { label: "Peak", min: 0, max: 1, step: 0.01 },
  gain: { label: "Gain", min: 0, max: 1, step: 0.01 },
  emphasis: { label: "Emphasis", min: 0, max: 1, step: 0.01 },
  mix: { label: "Mix", min: 0, max: 1, step: 0.01 },
  sensitivity: { label: "Sens", min: 0, max: 1, step: 0.01 },
  range: { label: "Range", min: 0, max: 1, step: 0.01 },
  resonance: { label: "Res", min: 0, max: 1, step: 0.01 },
  rate_hz: { label: "Rate", min: 0.02, max: 20, step: 0.01 },
  depth: { label: "Depth", min: 0, max: 1, step: 0.01 },
  feedback: { label: "Feedback", min: 0, max: 0.94, step: 0.01 },
  manual: { label: "Manual", min: 0, max: 1, step: 0.01 },
  sustain: { label: "Sustain", min: 0, max: 1, step: 0.01 },
  tone: { label: "Tone", min: 0, max: 1, step: 0.01 },
  level: { label: "Level", min: 0, max: 2, step: 0.01 },
  output: { label: "Output", min: 0, max: 1, step: 0.01 },
  distortion: { label: "Dist", min: 0, max: 1, step: 0.01 },
  time_ms: { label: "Time", min: 60, max: 700, step: 5 },
  repeats: { label: "Repeats", min: 0, max: 0.92, step: 0.01 },
  dwell: { label: "Dwell", min: 0, max: 1, step: 0.01 },
};

const stringControlOptions: Record<string, string[]> = {
  mode: ["standard", "custom"],
  wave: ["sine", "triangle", "square"],
};

function humanizeControlId(id: string) {
  return id.replaceAll("_", " ");
}

function formatControlValue(value: number) {
  return value >= 10 ? value.toFixed(0) : value.toFixed(2);
}

function RuntimePreview({ details }: { details: string }) {
  return (
    <section className="commandPanel">
      <div className="panelTitle">
        <h3>Web runtime</h3>
        <span>wasm sources</span>
      </div>
      <code>{details}</code>
    </section>
  );
}

function NumberField({ label, value, min, max, step, onChange }: { label: string; value: number; min?: number; max?: number; step?: number; onChange: (value: number) => void }) {
  return (
    <label className="field">
      <span>{label}</span>
      <input type="number" value={value} min={min} max={max} step={step} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}

function ReadOnlyField({ label, value }: { label: string; value: string }) {
  return (
    <div className="field">
      <span>{label}</span>
      <div className="readonlyField">{value}</div>
    </div>
  );
}

function AssetSelect({ label, value, options, onChange }: { label: string; value: string; options: { label: string; url: string }[]; onChange: (value: string) => void }) {
  return (
    <label className="field">
      <span>{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {options.map((option) => (
          <option key={option.url} value={option.url}>{option.label}</option>
        ))}
      </select>
    </label>
  );
}

function Switch({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="switch">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}
