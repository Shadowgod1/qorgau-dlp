"""
OCR-распознавание текста в сканах и изображениях.

Закон РК «О персональных данных»: ПДн должны выявляться в том числе в
отсканированных документах (фото паспорта, скан договора и т.п.).

Использует Tesseract (рус+каз+англ). При отсутствии движка деградирует
без ошибок — возвращает пустую строку (логируется один раз).
"""
import io
import logging

log = logging.getLogger("ocr")

IMAGE_EXTS = {".jpg", ".jpeg", ".png", ".bmp", ".tif", ".tiff", ".gif", ".webp"}

# Языки Tesseract: русский, казахский, английский
_LANGS = "rus+kaz+eng"
_MAX_CHARS = 4000

_available: bool | None = None


def _check_available() -> bool:
    global _available
    if _available is not None:
        return _available
    try:
        import pytesseract  # noqa
        from PIL import Image  # noqa
        pytesseract.get_tesseract_version()
        _available = True
    except Exception as e:
        log.warning("OCR недоступен (Tesseract не установлен?): %s", e)
        _available = False
    return _available


def ocr_image_bytes(data: bytes) -> str:
    """Распознаёт текст на изображении. Пустая строка при ошибке/недоступности."""
    if not _check_available():
        return ""
    try:
        import pytesseract
        from PIL import Image
        img = Image.open(io.BytesIO(data))
        # Небольшая нормализация: перевод в градации серого улучшает распознавание
        if img.mode not in ("L", "RGB"):
            img = img.convert("RGB")
        text = pytesseract.image_to_string(img, lang=_LANGS)
        return (text or "").strip()[:_MAX_CHARS]
    except Exception as e:
        log.debug("OCR failed: %s", e)
        return ""


def is_image(filename: str) -> bool:
    import os
    return os.path.splitext(filename or "")[1].lower() in IMAGE_EXTS
