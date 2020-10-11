#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec3 inTexCoords;
layout(location = 0) out vec4 outColor;
layout(location = 1) out vec3 outTexCoords;
layout(push_constant) uniform PushConstant {
    mat4 transform;
    vec4 glyphOffset;
    vec4 color;
} pushConstant;

void main() {
    gl_Position = pushConstant.transform * (pushConstant.glyphOffset + vec4(inPosition.x, inPosition.y, 0., 1.));
    outColor = pushConstant.color;
    outTexCoords = inTexCoords;
}