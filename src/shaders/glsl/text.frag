#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 inColor;
layout(location = 1) in vec3 inTexCoords;
layout(location = 0) out vec4 outColor;
layout(set = 0, binding = 0) uniform texture2DArray uColorTex;
layout(set = 0, binding = 1) uniform sampler uColorTexSampler;

void main() {
    float alpha = texture(sampler2DArray(uColorTex, uColorTexSampler), inTexCoords).r;
    vec4 texColor = vec4(1., 1., 1., alpha);
    outColor = inColor * texColor;
}