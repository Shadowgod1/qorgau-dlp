"""
Коллектор микрофона — запись аудио с микрофона компьютера.

Режимы работы:
1. Периодический — записывает фрагменты по MICROPHONE_DURATION секунд
   с паузой, равной интервалу записи.
2. Триггерный — сервер шлёт WS-команду {"type":"start_recording","agent_id":"..."}.

Формат: WAV (PCM 16-bit, mono, 16 kHz).
Файл отправляется на сервер через sender.upload_audio().

Правовое основание (ТК РК ст. 23): сотрудник уведомляется
о начале записи через всплывающее окно.
"""
import io
import logging
import threading
import time

import sounddevice as sd
import soundfile as sf
import numpy as np

from config import (
    MICROPHONE_DURATION,
    MICROPHONE_SAMPLE_RATE,
    MICROPHONE_CHANNELS,
    WARN_USER_ENABLED,
)

log = logging.getLogger("microphone")


class MicrophoneCollector:
    """Периодическая и триггерная запись с микрофона."""

    def __init__(
        self,
        sender,
        duration: int | None = None,
        sample_rate: int | None = None,
    ):
        self._sender = sender
        self._trigger = threading.Event()
        self._stop_flag = threading.Event()  # для остановки записи по команде
        self._recording = False

        self._duration = duration if duration is not None else MICROPHONE_DURATION
        self._sample_rate = sample_rate if sample_rate is not None else MICROPHONE_SAMPLE_RATE
        self._channels = MICROPHONE_CHANNELS

    # ── Публичный метод: одна запись по запросу ───────────────────────────────
    def take_now(self) -> None:
        """Записывает один фрагмент по внешнему триггеру."""
        try:
            self._record_and_upload()
            log.info("Triggered audio recording uploaded")
        except Exception as e:
            log.error("Triggered audio recording error: %s", e)

    def start_continuous(self) -> None:
        """Активирует непрерывную запись (по команде сервера)."""
        self._stop_flag.clear()
        self._trigger.set()

    def stop_continuous(self) -> None:
        """Останавливает непрерывную запись."""
        self._stop_flag.set()
        self._trigger.clear()

    # ── Основной цикл ────────────────────────────────────────────────────────
    def run(self) -> None:
        """Запускается в daemon-потоке из agent.py."""
        while True:
            try:
                self._record_and_upload()
            except Exception as e:
                log.error("Microphone recording error: %s", e)

            # Ждём таймер ИЛИ триггер (кто наступит первым)
            self._trigger.wait(timeout=self._duration * 2)
            self._trigger.clear()

            # Если был триггер на непрерывную запись — пишем без пауз
            while not self._stop_flag.is_set() and self._trigger.is_set():
                try:
                    self._record_and_upload()
                except Exception as e:
                    log.error("Continuous microphone recording error: %s", e)

    # ── Запись и загрузка ────────────────────────────────────────────────────
    def _record_and_upload(self) -> None:
        if self._recording:
            return  # Уже идёт запись
        self._recording = True

        try:
            # Уведомление сотрудника (ТК РК ст. 23)
            if WARN_USER_ENABLED:
                from notifier import notify
                notify(
                    "Qorgau DLP: выполняется запись аудио с микрофона.\n"
                    "Данная функция активирована администратором безопасности.",
                )

            log.info(
                "Recording audio: %d sec @ %d Hz, %d ch",
                self._duration, self._sample_rate, self._channels,
            )

            # Запись через sounddevice (блокирующая)
            audio_data = sd.rec(
                frames=int(self._duration * self._sample_rate),
                samplerate=self._sample_rate,
                channels=self._channels,
                dtype="int16",
                blocking=True,
            )

            if audio_data is None or len(audio_data) == 0:
                log.warning("Empty audio buffer — no microphone?")
                return

            # Кодирование в WAV (in-memory)
            buf = io.BytesIO()
            sf.write(
                buf, audio_data, self._sample_rate,
                format="WAV", subtype="PCM_16",
            )
            wav_bytes = buf.getvalue()

            log.info("Audio recorded: %d bytes, uploading...", len(wav_bytes))
            self._sender.upload_audio(wav_bytes, self._duration)

        finally:
            self._recording = False

    # ── Проверка доступности микрофона ────────────────────────────────────────
    @staticmethod
    def is_available() -> bool:
        """Проверяет наличие входного аудиоустройства."""
        try:
            devices = sd.query_devices()
            for d in devices if isinstance(devices, list) else [devices]:
                if d.get("max_input_channels", 0) > 0:
                    return True
        except Exception:
            pass
        return False
