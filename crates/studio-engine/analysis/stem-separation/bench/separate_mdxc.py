"""Run an explicit MDXC checkpoint/config pair with audio-separator 0.48.0."""
import argparse
from importlib.metadata import version
from pathlib import Path

from audio_separator.separator import Separator


class LocalMdxc(Separator):
    def __init__(self, checkpoint, config, output):
        self.checkpoint = checkpoint
        self.config = config
        super().__init__(model_file_dir=str(checkpoint.parent), output_dir=str(output),
                         output_format="WAV", use_soundfile=True,
                         normalization_threshold=1.0, amplification_threshold=0.0,
                         mdxc_params={"batch_size": 1, "overlap": 2})

    def download_model_files(self, model_filename):
        if model_filename != self.checkpoint.name:
            raise ValueError(f"unexpected model: {model_filename}")
        return model_filename, "MDXC", model_filename, str(self.checkpoint), str(self.config)


def main():
    parser = argparse.ArgumentParser()
    for name in ("input", "checkpoint", "config", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    if version("audio-separator") != "0.48.0":
        parser.error("this runner targets audio-separator 0.48.0")
    args.output.mkdir()
    separator = LocalMdxc(args.checkpoint.resolve(strict=True), args.config.resolve(strict=True),
                          args.output.resolve())
    separator.load_model(args.checkpoint.name)
    if not separator.separate(str(args.input.resolve(strict=True))):
        raise RuntimeError("no stems produced")


if __name__ == "__main__":
    main()
