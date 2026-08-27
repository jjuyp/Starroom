#include <libraw/libraw.h>

// This product includes DNG technology under license by Adobe.
// See NOTICE.md and docs/17_THIRD_PARTY_PROVENANCE.md.

#include <chrono>
#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include <cstring>

extern "C" {

struct SrRawResult {
  uint32_t width;
  uint32_t height;
  uint32_t raw_width;
  uint32_t raw_height;
  uint32_t active_width;
  uint32_t active_height;
  uint32_t left_margin;
  uint32_t top_margin;
  int32_t orientation;
  uint32_t filters;
  uint32_t colors;
  uint32_t dng_version;
  uint32_t black;
  uint32_t maximum;
  uint32_t cblack[4];
  float camera_multipliers[4];
  float pre_multipliers[4];
  float cam_xyz[12];
  uint32_t dng_parsed_fields[2];
  uint16_t dng_illuminants[2];
  float dng_calibration[32];
  float dng_color_matrix[24];
  float dng_forward_matrix[24];
  uint8_t xtrans[36];
  uint8_t sensor_layout;
  uint8_t used_half_size;
  uint16_t reserved;
  double unpack_milliseconds;
  double process_milliseconds;
  float focal_length_mm;
  float aperture;
  float focus_distance_m;
  char make[64];
  char model[64];
  char lens_make[128];
  char lens_model[128];
  char decoder[128];
  uint16_t *rgb16;
  size_t rgb16_length;
};

static void sr_copy_text(char *destination, size_t capacity, const char *source) {
  if (!destination || capacity == 0) {
    return;
  }
  if (!source) {
    destination[0] = '\0';
    return;
  }
  std::strncpy(destination, source, capacity - 1);
  destination[capacity - 1] = '\0';
}

static int sr_fail(int code, const char *stage, char *error, size_t capacity) {
  if (error && capacity > 0) {
    const char *detail = libraw_strerror(code);
    if (!detail) {
      detail = "unknown LibRaw error";
    }
    std::snprintf(error, capacity, "%s: %s (%d)", stage, detail, code);
  }
  return code == LIBRAW_SUCCESS ? LIBRAW_UNSPECIFIED_ERROR : code;
}

int sr_libraw_decode_buffer(const uint8_t *bytes, size_t byte_length,
                            int half_size, SrRawResult *result, char *error,
                            size_t error_capacity) {
  if (!bytes || byte_length == 0 || !result) {
    return sr_fail(LIBRAW_IO_ERROR, "input", error, error_capacity);
  }
  std::memset(result, 0, sizeof(*result));
  LibRaw processor;
  int status = processor.open_buffer(const_cast<uint8_t *>(bytes), byte_length);
  if (status != LIBRAW_SUCCESS) {
    return sr_fail(status, "open", error, error_capacity);
  }

  const auto unpack_start = std::chrono::steady_clock::now();
  status = processor.unpack();
  const auto unpack_end = std::chrono::steady_clock::now();
  if (status != LIBRAW_SUCCESS) {
    return sr_fail(status, "sensor unpack", error, error_capacity);
  }

  const libraw_data_t &data = processor.imgdata;
  if (!data.rawdata.raw_image && !data.rawdata.color3_image &&
      !data.rawdata.color4_image) {
    return sr_fail(LIBRAW_DATA_ERROR, "sensor buffer", error, error_capacity);
  }

  result->raw_width = data.sizes.raw_width;
  result->raw_height = data.sizes.raw_height;
  result->active_width = data.sizes.width;
  result->active_height = data.sizes.height;
  result->left_margin = data.sizes.left_margin;
  result->top_margin = data.sizes.top_margin;
  result->orientation = data.sizes.flip;
  result->filters = data.idata.filters;
  result->colors = static_cast<uint32_t>(data.idata.colors);
  result->dng_version = data.idata.dng_version;
  result->black = data.color.black;
  result->maximum = data.color.maximum;
  for (size_t index = 0; index < 4; ++index) {
    result->cblack[index] = data.color.cblack[index];
    result->camera_multipliers[index] = data.color.cam_mul[index];
    result->pre_multipliers[index] = data.color.pre_mul[index];
    for (size_t xyz = 0; xyz < 3; ++xyz) {
      result->cam_xyz[index * 3 + xyz] = data.color.cam_xyz[index][xyz];
    }
  }
  for (size_t set = 0; set < 2; ++set) {
    const libraw_dng_color_t &dng = data.color.dng_color[set];
    result->dng_parsed_fields[set] = dng.parsedfields;
    result->dng_illuminants[set] = dng.illuminant;
    for (size_t row = 0; row < 4; ++row) {
      for (size_t column = 0; column < 4; ++column) {
        result->dng_calibration[set * 16 + row * 4 + column] =
            dng.calibration[row][column];
      }
      for (size_t xyz = 0; xyz < 3; ++xyz) {
        result->dng_color_matrix[set * 12 + row * 3 + xyz] =
            dng.colormatrix[row][xyz];
      }
    }
    for (size_t xyz = 0; xyz < 3; ++xyz) {
      for (size_t channel = 0; channel < 4; ++channel) {
        result->dng_forward_matrix[set * 12 + xyz * 4 + channel] =
            dng.forwardmatrix[xyz][channel];
      }
    }
  }
  for (size_t row = 0; row < 6; ++row) {
    for (size_t column = 0; column < 6; ++column) {
      result->xtrans[row * 6 + column] =
          static_cast<uint8_t>(data.idata.xtrans[row][column]);
    }
  }
  result->sensor_layout = data.idata.filters == 9 ? 2 :
      (data.idata.filters != 0 ? 1 : 3);
  result->used_half_size = half_size ? 1 : 0;
  result->unpack_milliseconds =
      std::chrono::duration<double, std::milli>(unpack_end - unpack_start)
          .count();
  sr_copy_text(result->make, sizeof(result->make), data.idata.make);
  sr_copy_text(result->model, sizeof(result->model), data.idata.model);
  result->focal_length_mm = data.other.focal_len;
  result->aperture = data.other.aperture;
  result->focus_distance_m = data.lens.makernotes.MinFocusDistance;
  sr_copy_text(result->lens_make, sizeof(result->lens_make), data.lens.LensMake);
  sr_copy_text(result->lens_model, sizeof(result->lens_model), data.lens.Lens);
  if (result->lens_model[0] == '\0') {
    sr_copy_text(result->lens_model, sizeof(result->lens_model), data.lens.makernotes.Lens);
  }
  libraw_decoder_info_t decoder_info{};
  if (processor.get_decoder_info(&decoder_info) == LIBRAW_SUCCESS) {
    sr_copy_text(result->decoder, sizeof(result->decoder),
                 decoder_info.decoder_name);
  }

  processor.imgdata.params.output_bps = 16;
  // Preserve linear camera RGB after LibRaw's mature sensor scaling, As-Shot WB
  // and demosaic. Starroom's Rust CameraProfileResolver owns the explicit
  // camera RGB -> XYZ -> Rec.2020/D65 color stage shared by Preview and Export.
  processor.imgdata.params.output_color = 0;
  processor.imgdata.params.gamm[0] = 1.0;
  processor.imgdata.params.gamm[1] = 1.0;
  processor.imgdata.params.no_auto_bright = 1;
  processor.imgdata.params.use_auto_wb = 0;
  processor.imgdata.params.use_camera_wb = 1;
  processor.imgdata.params.user_qual = 3; // AHD; X-Trans selects LibRaw's mature path.
  processor.imgdata.params.half_size = half_size ? 1 : 0;
  processor.imgdata.params.user_flip = -1;
  for (size_t row = 0; row < 3; ++row) {
    for (size_t column = 0; column < 4; ++column) {
      processor.imgdata.color.rgb_cam[row][column] = row == column ? 1.0f : 0.0f;
    }
  }

  const auto process_start = std::chrono::steady_clock::now();
  status = processor.dcraw_process();
  if (status != LIBRAW_SUCCESS) {
    return sr_fail(status, "demosaic/process", error, error_capacity);
  }
  int memory_status = LIBRAW_SUCCESS;
  libraw_processed_image_t *image =
      processor.dcraw_make_mem_image(&memory_status);
  const auto process_end = std::chrono::steady_clock::now();
  if (!image || memory_status != LIBRAW_SUCCESS) {
    if (image) {
      LibRaw::dcraw_clear_mem(image);
    }
    return sr_fail(memory_status, "memory image", error, error_capacity);
  }
  if (image->type != LIBRAW_IMAGE_BITMAP || image->bits != 16 ||
      image->colors != 3) {
    LibRaw::dcraw_clear_mem(image);
    return sr_fail(LIBRAW_DATA_ERROR, "unexpected processed image", error,
                   error_capacity);
  }

  const size_t sample_count = static_cast<size_t>(image->width) *
                              static_cast<size_t>(image->height) * 3;
  auto *copy = static_cast<uint16_t *>(std::malloc(sample_count * sizeof(uint16_t)));
  if (!copy) {
    LibRaw::dcraw_clear_mem(image);
    return sr_fail(LIBRAW_UNSUFFICIENT_MEMORY, "copy", error, error_capacity);
  }
  std::memcpy(copy, image->data, sample_count * sizeof(uint16_t));
  result->width = image->width;
  result->height = image->height;
  result->rgb16 = copy;
  result->rgb16_length = sample_count;
  result->process_milliseconds =
      std::chrono::duration<double, std::milli>(process_end - process_start)
          .count();
  LibRaw::dcraw_clear_mem(image);
  return LIBRAW_SUCCESS;
}

void sr_libraw_free(uint16_t *buffer) { std::free(buffer); }

const char *sr_libraw_version() { return libraw_version(); }

} // extern "C"
