import { BaseShape } from './base-shape';
import { Circle } from './circle';
import { Rectangle } from './rectangle';

function printShapeInfo(shape: BaseShape): void {
  console.log(shape.describe());
  console.log(`  Area: ${shape.getArea()}`);
  console.log(`  Perimeter: ${shape.getPerimeter()}`);
}

const shapes: BaseShape[] = [new Circle(5), new Rectangle(4, 6)];
for (const shape of shapes) {
  printShapeInfo(shape);
}
